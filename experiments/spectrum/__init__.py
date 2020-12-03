from __future__ import annotations

import asyncio
import math

from abc import ABC, abstractmethod
from contextlib import contextmanager
from dataclasses import dataclass, field
from itertools import chain, starmap, product
from pathlib import Path
from subprocess import check_call
from tempfile import NamedTemporaryFile, TemporaryDirectory
from typing import NewType, Dict, Any, Optional, Type, List, Mapping, Iterator

import asyncssh

from halo import Halo
from tenacity import stop_after_attempt, wait_fixed, AsyncRetrying

import experiments.system as system

from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import InstanceType, SHA, AWS_REGION

BuildProfile = NewType("BuildProfile", str)
Bytes = NewType("Bytes", int)


# Need to update install.sh to change this
MAX_WORKERS_PER_MACHINE = 10


@dataclass
class Setting(system.Setting):
    publisher: Machine
    workers: List[Machine]
    clients: List[Machine]

    async def additional_setup(self):
        with Halo("[infrastructure] starting etcd") as spinner:
            await self.publisher.ssh.run(
                "envsubst '$HOSTNAME' "
                '    < "$HOME/config/etcd.template" '
                "    | sudo tee /etc/default/etcd "
                "    > /dev/null",
                check=True,
            )
            await self.publisher.ssh.run("sudo systemctl start etcd", check=True)
            # Make sure etcd is healthy
            async for attempt in AsyncRetrying(
                wait=wait_fixed(2), stop=stop_after_attempt(20)
            ):
                with attempt:
                    await self.publisher.ssh.run(
                        (
                            "ETCDCTL_API=3 etcdctl "
                            f"--endpoints {self.publisher.hostname}:2379 "
                            "endpoint health"
                        ),
                        check=True,
                    )
            spinner.succeed("[infrastructure] etcd healthy")


@dataclass(order=True, frozen=True)
class Environment(system.Environment):
    instance_type: InstanceType
    client_machines: int
    worker_machines: int

    @property
    def total_machines(self) -> int:
        return self.client_machines + self.worker_machines + 1

    def to_setup(self, machines: List[Machine]) -> Setting:
        assert len(machines) == self.total_machines
        return Setting(
            publisher=machines[0],
            workers=machines[1 : self.worker_machines + 1],
            clients=machines[-self.client_machines :],
        )


class Protocol(ABC):
    @property
    @abstractmethod
    def flag(self) -> str:
        ...

    @classmethod
    @abstractmethod
    def _from_dict(cls, data: Dict[str, Any]) -> Protocol:
        ...

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Protocol:
        assert len(data) == 1
        key = next(iter(data.keys()))
        subclasses = {cls.__name__: cls for cls in Protocol.__subclasses__()}
        subcls: Optional[Type[Protocol]] = subclasses.get(key, None)
        if subcls is None:
            raise ValueError(
                f"Invalid protocol {data}. Expected one of {list(subclasses.keys())}."
            )
        return subcls._from_dict(data[key])  # pylint: disable=protected-access


@dataclass(frozen=True)
class Symmetric(Protocol):
    security: Bytes = field(default=Bytes(16))

    @property
    def flag(self) -> str:
        return f"--security {self.security}"

    @classmethod
    def _from_dict(cls, data: Dict[str, Any]) -> Symmetric:
        return cls(**data)


@dataclass(frozen=True)
class Insecure(Protocol):
    parties: int = field(default=2)

    @property
    def flag(self) -> str:
        return "--no-security"

    @classmethod
    def _from_dict(cls, data: Dict[str, Any]) -> Insecure:
        return cls(**data)


@dataclass(frozen=True)
class SeedHomomorphic(Protocol):
    parties: int

    @property
    def flag(self) -> str:
        return "--security-multi-key 16"

    @classmethod
    def _from_dict(cls, data: Dict[str, Any]) -> SeedHomomorphic:
        return cls(**data)


async def _install_spectrum_config(machine: Machine, spectrum_config: Dict[str, Any]):
    spectrum_config_str = "\n".join([f"{k}={v}" for k, v in spectrum_config.items()])
    with NamedTemporaryFile() as tmp:
        tmp.write(spectrum_config_str.encode("utf8"))
        tmp.flush()
        await asyncssh.scp(tmp.name, (machine.ssh, "/tmp/spectrum.conf"))
    await machine.ssh.run(
        "sudo install -m 644 /tmp/spectrum.conf /etc/spectrum.conf", check=True
    )


async def _prepare_worker(
    machine: Machine,
    group: int,
    worker_start_idx: int,
    num_workers: int,
    etcd_env: Mapping[str, str],
):
    spectrum_config: Dict[str, Any] = {
        "SPECTRUM_WORKER_GROUP": group,
        "SPECTRUM_LEADER_GROUP": group,
        "SPECTRUM_WORKER_START_INDEX": worker_start_idx,
        **etcd_env,
    }
    await _install_spectrum_config(machine, spectrum_config)

    await machine.ssh.run(
        f"sudo systemctl start spectrum-worker@{{1..{num_workers}}}", check=True
    )
    await machine.ssh.run("sudo systemctl start spectrum-leader", check=True)


async def _prepare_client(
    machine: Machine, client_range: slice, etcd_env: Dict[str, Any]
):
    await _install_spectrum_config(machine, etcd_env)
    await machine.ssh.run(
        f"sudo systemctl start viewer@{{{client_range.start}..{client_range.stop}}}",
        check=True,
    )


async def _execute_experiment(
    publisher: Machine, etcd_env: Dict[str, Any]
) -> Milliseconds:
    await _install_spectrum_config(publisher, etcd_env)
    await publisher.ssh.run(
        "sudo systemctl start spectrum-publisher --wait", check=True
    )

    result = await publisher.ssh.run(
        "journalctl --unit spectrum-publisher "
        "    | grep -o 'Elapsed time: .*' "
        "    | sed 's/Elapsed time: \\(.*\\)ms/\\1/'",
        check=True,
    )
    result = Milliseconds(int(result.stdout.strip()))

    return result


@dataclass(frozen=True)
class Experiment(system.Experiment):
    clients: int
    channels: int
    message_size: Bytes
    instance_type: InstanceType = field(default=InstanceType("c5.9xlarge"))
    clients_per_machine: int = field(default=250)
    workers_per_machine: int = field(default=1)  # TODO: better default
    worker_machines_per_group: int = field(default=1)
    protocol: Protocol = field(default=Symmetric())

    @property
    def groups(self) -> int:
        if isinstance(self.protocol, Symmetric):
            return 2
        if isinstance(self.protocol, Insecure):
            return self.protocol.parties
        if isinstance(self.protocol, SeedHomomorphic):
            return self.protocol.parties
        raise TypeError(
            f"Invalid protocol {self.protocol}. "
            "Expected one of Symmetric, Insecure, SeedHomomorphic"
        )

    @property
    def group_size(self) -> int:
        return self.workers_per_machine * self.worker_machines_per_group

    def to_environment(self) -> Environment:
        client_machines = math.ceil(self.clients / self.clients_per_machine)
        worker_machines = self.worker_machines_per_group * self.groups
        return Environment(
            instance_type=self.instance_type,
            worker_machines=worker_machines,
            client_machines=client_machines,
        )

    @classmethod
    def from_dict(cls, data) -> Experiment:
        protocol = data.pop("protocol", None)
        if protocol is not None:
            data["protocol"] = Protocol.from_dict(protocol)
        return cls(**data)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        try:
            publisher = setting.publisher
            workers = setting.workers
            clients = setting.clients

            etcd_url = f"etcd://{publisher.hostname}:2379"
            etcd_env = {"SPECTRUM_CONFIG_SERVER": etcd_url}

            spinner.text = "[experiment] setting up"
            # don't let this same output confuse us if we run on this machine again
            await publisher.ssh.run(
                "sudo journalctl --rotate && sudo journalctl --vacuum-time=1s",
                check=True,
            )
            # ensure a blank slate
            await publisher.ssh.run(
                "ETCDCTL_API=3 etcdctl --endpoints localhost:2379 del --prefix ''",
                check=True,
            )
            # can't use ssh.run(env=...) because the SSH server doesn't like it.
            await publisher.ssh.run(
                f"SPECTRUM_CONFIG_SERVER={etcd_url} "
                "/home/ubuntu/spectrum/setup"
                f"    {self.protocol.flag}"
                f"    --channels {self.channels}"
                f"    --clients {self.clients}"
                f"    --group-size {self.group_size}"
                f"    --groups {self.groups}"
                f"    --message-size {self.message_size}",
                check=True,
                timeout=15,
            )

            spinner.text = "[experiment] starting workers and clients"
            assert self.workers_per_machine <= MAX_WORKERS_PER_MACHINE
            await asyncio.gather(
                *[
                    _prepare_worker(
                        worker,
                        group + 1,
                        machine_idx * self.workers_per_machine,
                        self.workers_per_machine,
                        etcd_env,
                    )
                    for (machine_idx, group), worker in zip(
                        product(
                            range(self.worker_machines_per_group), range(self.groups)
                        ),
                        workers,
                    )
                ]
            )

            # Full client count at every machine except the last
            cpm = self.clients_per_machine
            client_counts = starmap(
                slice,
                zip(
                    range(1, self.clients, cpm),
                    chain(range(cpm, self.clients, cpm), [self.clients]),
                ),
            )
            await asyncio.gather(
                *[
                    _prepare_client(client, client_range, etcd_env)
                    for client, client_range in zip(clients, client_counts)
                ]
            )

            spinner.text = "[experiment] running"
            time = await asyncio.wait_for(
                _execute_experiment(publisher, etcd_env), timeout=60.0
            )
            return Result(self, time)
        finally:
            spinner.text = "[experiment] shutting everything down"
            shutdowns = []
            for worker in workers:
                shutdowns.append(
                    worker.ssh.run("sudo systemctl stop spectrum-leader", check=False)
                )
                shutdowns.append(
                    worker.ssh.run(
                        "sudo systemctl stop 'spectrum-worker@*'", check=False
                    )
                )
            for client in clients:
                shutdowns.append(
                    client.ssh.run("sudo systemctl stop 'viewer@*'", check=False)
                )
            shutdowns.append(
                publisher.ssh.run("sudo systemctl stop spectrum-publisher", check=False)
            )
            await asyncio.gather(*shutdowns)


@dataclass
class PackerConfig(system.PackerConfig):
    sha: SHA
    git_root: Path
    profile: BuildProfile
    instance_type: InstanceType

    @contextmanager
    def make_packer_args(self) -> Iterator[Dict[str, str]]:
        with TemporaryDirectory() as tmpdir:
            src_path = Path(tmpdir) / "spectrum-src.tar.gz"
            cmd = (
                "git archive --format tar.gz".split(" ")
                + ["--output", str(src_path)]
                + "--prefix spectrum/".split(" ")
                + [str(self.sha)]
            )
            check_call(cmd, cwd=self.git_root)

            yield {
                "sha": self.sha,
                "src_archive": str(src_path),
                "profile": self.profile,
                "region": AWS_REGION,
                "instance_type": self.instance_type,
            }

    def matches(self, build: Dict[str, str]) -> bool:
        return (
            self.instance_type == InstanceType(build["instance_type"])
            and self.sha == SHA(build["sha"])
            and self.profile == BuildProfile(build["profile"])
        )

    # def make_tf_vars(self, build: packer.Build) -> Dict[str, str]:
    #     return {
    #         "ami": build.ami,
    #         "region": AWS_REGION,
    #         "instance_type": self.instance_type,
    #     }


SPECTRUM = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path("/"),
)
