from __future__ import annotations

import asyncio
import math
import re

from abc import ABC, abstractmethod
from contextlib import contextmanager
from dataclasses import dataclass, field
from itertools import chain, starmap, product
from operator import attrgetter
from pathlib import Path
from subprocess import check_call
from tempfile import NamedTemporaryFile, TemporaryDirectory
from typing import (
    NewType,
    Dict,
    Any,
    Optional,
    Type,
    List,
    Mapping,
    Iterator,
    Union,
    Tuple,
)
from statistics import mean

import asyncssh

from halo import Halo
from tenacity import stop_after_attempt, wait_fixed, AsyncRetrying

from experiments import system, packer

from experiments.system import Result, Machine, Milliseconds
from experiments.cloud import InstanceType, SHA, AWS_REGION
from experiments.util import Bytes

BuildProfile = NewType("BuildProfile", str)


# Need to update install.sh to change this
MAX_WORKERS_PER_MACHINE = 10

EXPERIMENT_TIMEOUT = 60.0


@dataclass
class Setting(system.Setting):
    publisher: Machine
    workers: List[Machine]
    clients: List[Machine]

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        result["publisher"] = tf_data["publisher"]
        for idx, worker in enumerate(tf_data["workers"]):
            result[("worker", idx)] = worker
        for idx, client in enumerate(tf_data["clients"]):
            result[("client", idx)] = client
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        publisher = None
        workers = []
        clients = []
        for ident, machine in machines.items():
            if ident == "publisher":
                publisher = machine
            elif ident[0] == "worker":
                workers.append(machine)
            elif ident[0] == "client":
                clients.append(machine)
            else:
                raise ValueError(f"Invalid identifier [{ident}]")
        if publisher is None:
            raise ValueError("Missing publisher.")
        return cls(publisher=publisher, workers=workers, clients=clients)

    async def additional_setup(self):
        with Halo("[infrastructure] starting etcd") as spinner:
            await self.publisher.ssh.run(
                "envsubst '$HOSTNAME' "
                '    < "$HOME/config/etcd.template" '
                "    | sudo tee /etc/default/etcd "
                "    > /dev/null",
                check=True,
            )
            await self.publisher.ssh.run("sudo systemctl restart etcd", check=True)
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

    def make_tf_vars(self, build: packer.Build) -> Dict[str, Any]:
        return {
            "ami": build.ami,
            "region": build.region,
            "instance_type": self.instance_type,
            "client_machine_count": self.client_machines,
            "worker_machine_count": self.worker_machines,
        }

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "ami": "null",
            "instance_type": "null",
            "client_machine_count": 0,
            "worker_machine_count": 0,
        }


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

    # don't let this same output confuse us if we run on this machine again
    await machine.ssh.run(
        "sudo journalctl --rotate && sudo journalctl --vacuum-time=1s", check=True,
    )

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


@dataclass(frozen=True)
class Experiment(system.Experiment):
    clients: int
    channels: int
    message_size: Bytes
    instance_type: InstanceType = InstanceType("c5.9xlarge")
    clients_per_machine: int = 250
    workers_per_machine: int = 1  # TODO: better default
    worker_machines_per_group: int = 1
    protocol: Protocol = Symmetric()

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

    async def _fetch_timing(self, worker: Machine) -> Result:
        """Timing for one worker.

        We look across all worker processes and get the best intermediate result from each.
        We then report the *total* QPS.
        """
        total_qps = 0
        min_time = None
        for worker_process in range(1, self.workers_per_machine + 1):
            cmd_result = await worker.ssh.run(
                f"journalctl --unit spectrum-worker@{worker_process}"
                r"    | grep -Eo '[0-9]+ clients processed in time [0-9]+ms \([0-9]+ qps\)'",
            )
            process_results = []
            for line in cmd_result.stdout.split("\n"):
                match = re.match(
                    r"([0-9]+) clients processed in time ([0-9]+)ms \([0-9]+ qps\)",
                    line,
                )
                if not match:
                    continue
                clients, time = match.groups()
                process_results.append(
                    Result(
                        experiment=self,
                        queries=int(clients),
                        time=Milliseconds(int(time)),
                    )
                )
            if not process_results:
                continue
            best_result = max(process_results, key=attrgetter("qps"))
            total_qps += best_result.qps
            if min_time is None:
                min_time = best_result.time
            else:
                min_time = min(min_time, best_result.time)
        if min_time is None:
            raise RuntimeError("No successful runs.")
        result = Result(
            experiment=self,
            queries=int(total_qps * int(min_time) / 1000),
            time=min_time,
        )
        assert result.qps - total_qps < 0.1
        return result

    async def _execute_experiment(
        self, publisher: Machine, workers: List[Machine], etcd_env: Dict[str, Any]
    ) -> Result:
        await _install_spectrum_config(publisher, etcd_env)
        timeout = EXPERIMENT_TIMEOUT - 2  # give some cleanup time
        try:
            await publisher.ssh.run(
                "sudo systemctl start spectrum-publisher --wait",
                check=True,
                timeout=timeout,
            )
        except asyncssh.process.TimeoutError:
            pass

        results = await asyncio.gather(*map(self._fetch_timing, workers))
        # We now have the total QPS per machine; let's aggregate.
        # Divide by self.groups so we don't double-count.
        total_qps = sum(map(attrgetter("qps"), results)) / self.groups
        min_time = min(map(attrgetter("time"), results))
        return Result(
            experiment=self,
            time=min_time,
            queries=int(total_qps * int(min_time) / 1000),
        )

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
            return await asyncio.wait_for(
                self._execute_experiment(publisher, workers, etcd_env),
                timeout=EXPERIMENT_TIMEOUT,
            )
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


@dataclass(frozen=True)
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

    @classmethod
    def from_args(cls, args: Any, environment: Environment) -> PackerConfig:
        return PackerConfig(
            sha=args.sha,
            git_root=args.git_root,
            profile=args.profile,
            instance_type=environment.instance_type,
        )


# pylint: disable=duplicate-code
SPECTRUM = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path(__file__).parent,
)
# pylint: enable=duplicate-code
