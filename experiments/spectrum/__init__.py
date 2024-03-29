from __future__ import annotations

import asyncio
import math
import re

from abc import ABC, abstractmethod
from contextlib import contextmanager
from dataclasses import dataclass, field
from itertools import chain, starmap, product, cycle
from operator import attrgetter, itemgetter
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
from experiments.cloud import DEFAULT_INSTANCE_TYPE, InstanceType, SHA, AWS_REGION
from experiments.util import Bytes

BuildProfile = NewType("BuildProfile", str)


# Need to update install.sh to change this
MAX_WORKERS_PER_MACHINE = 10

EXPERIMENT_TIMEOUT = 60.0
EXPERIMENT_LONG_TIMEOUT = 1000


@dataclass
class Setting(system.Setting):
    publisher: Machine
    workers_east: List[Machine]
    workers_west: List[Machine]
    clients: List[Machine]

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        result["publisher"] = tf_data["publisher"]
        for idx, worker in enumerate(tf_data["workers_east"]):
            result[("worker_east", idx)] = worker
        for idx, worker in enumerate(tf_data["workers_west"]):
            result[("worker_west", idx)] = worker
        for idx, client in enumerate(tf_data["clients"]):
            result[("client", idx)] = client
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        publisher = None
        workers_east = []
        workers_west = []
        clients = []
        for ident, machine in machines.items():
            if ident == "publisher":
                publisher = machine
            elif ident[0] == "worker_east":
                workers_east.append(machine)
            elif ident[0] == "worker_west":
                workers_west.append(machine)
            elif ident[0] == "client":
                clients.append(machine)
            else:
                raise ValueError(f"Invalid identifier [{ident}]")
        if publisher is None:
            raise ValueError("Missing publisher.")
        return cls(
            publisher=publisher,
            workers_east=workers_east,
            workers_west=workers_west,
            clients=clients,
        )

    @property
    def all_workers(self):
        return self.workers_east + self.workers_west

    async def additional_setup(self):
        with Halo("[infrastructure] starting etcd") as spinner:
            await self.publisher.ssh.run(
                f"HOSTNAME={self.publisher.hostname} "
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
    worker_machines_east: int
    worker_machines_west: int

    @property
    def total_machines(self) -> int:
        return (
            self.client_machines
            + self.worker_machines_east
            + self.worker_machines_west
            + 1
        )

    def make_tf_vars(
        self, _build: Optional[packer.Build], build_args: BuildArgs
    ) -> Dict[str, Any]:
        tf_vars = {
            "instance_type": self.instance_type,
            "client_machine_count": self.client_machines,
            "worker_machine_east_count": self.worker_machines_east,
            "worker_machine_west_count": self.worker_machines_west,
            "region": AWS_REGION,
            "sha": build_args.sha,
        }
        return tf_vars

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "instance_type": DEFAULT_INSTANCE_TYPE,
            "client_machine_count": 0,
            "worker_machine_east_count": 0,
            "worker_machine_west_count": 0,
            "sha": "null",
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
class SymmetricPub(Protocol):
    security: Bytes = field(default=Bytes(16))

    @property
    def flag(self) -> str:
        return f"--security {self.security} --public"

    @classmethod
    def _from_dict(cls, data: Dict[str, Any]) -> SymmetricPub:
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
    leader: bool,
):
    spectrum_config: Dict[str, Any] = {
        "SPECTRUM_WORKER_GROUP": group,
        "SPECTRUM_LEADER_GROUP": group,
        "SPECTRUM_WORKER_START_INDEX": worker_start_idx,
        "SPECTRUM_LOG_LEVEL": "debug",
        "SPECTRUM_TLS_CA": "/home/ubuntu/spectrum/data/ca.crt",
        "SPECTRUM_TLS_KEY": "/home/ubuntu/spectrum/data/server.key",
        "SPECTRUM_TLS_CERT": "/home/ubuntu/spectrum/data/server.crt",
        **etcd_env,
    }
    await _install_spectrum_config(machine, spectrum_config)

    # don't let this same output confuse us if we run on this machine again
    await machine.ssh.run(
        "sudo journalctl --rotate && sudo journalctl --vacuum-time=1s",
        check=True,
    )

    await machine.ssh.run(
        f"sudo systemctl start spectrum-worker@{{1..{num_workers}}}", check=True
    )
    if leader:
        await machine.ssh.run(f"sudo systemctl start spectrum-leader", check=True)


def distribute(balls: int, balls_per_bin: int):
    counts = [balls_per_bin] * (balls // balls_per_bin)
    counts.append(balls % balls_per_bin)
    return counts


@dataclass(frozen=True)
class Experiment(system.Experiment):
    clients: int
    channels: int
    message_size: Bytes
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    clients_per_machine: int = None
    workers_per_machine: int = 4
    worker_machines_per_group: int = 1
    protocol: Protocol = Symmetric()
    hammer: bool = True
    expected_runtime: int = None

    @property
    def groups(self) -> int:
        if isinstance(self.protocol, Symmetric):
            return 2
        if isinstance(self.protocol, SymmetricPub):
            return 2
        if isinstance(self.protocol, SeedHomomorphic):
            return self.protocol.parties
        raise TypeError(
            f"Invalid protocol {self.protocol}. "
            "Expected one of Symmetric, SymmetricPub, SeedHomomorphic"
        )

    @property
    def cpm(self):
        if self.clients_per_machine is not None:
            return self.clients_per_machine
        elif self.hammer:
            return 8
        else:
            return 500

    @property
    def group_size(self) -> int:
        return self.workers_per_machine * self.worker_machines_per_group

    def to_environment(self) -> Environment:
        client_machines = math.ceil(self.clients / self.cpm)
        worker_machines_east = self.worker_machines_per_group * ((self.groups + 1) // 2)
        worker_machines_west = self.worker_machines_per_group * (self.groups // 2)
        return Environment(
            instance_type=self.instance_type,
            worker_machines_east=worker_machines_east,
            worker_machines_west=worker_machines_west,
            client_machines=client_machines,
        )

    @classmethod
    def from_dict(cls, data) -> Experiment:
        protocol = data.pop("protocol", None)
        if protocol is not None:
            data["protocol"] = Protocol.from_dict(protocol)
        return cls(**data)

    async def _fetch_timing(
        self, worker: Machine
    ) -> Optional[Tuple[int, Milliseconds]]:
        """Timing for one worker.

        We look across all worker processes and get intermediate results from each.
        We then report the *total* QPS.
        """
        total_qps = 0
        max_time = None
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
                if int(time) < 1000:  # early results often noisy
                    continue
                process_results.append((int(clients), int(time)))
            if not process_results:
                continue
            best_queries, best_time = max(process_results, key=lambda x: x[0] / x[1])
            best_qps = 1000 * best_queries / best_time
            total_qps += best_qps
            if max_time is None:
                max_time = best_time
            else:
                max_time = max(max_time, best_time)
        if max_time is None:
            return None
        queries = int(total_qps * (int(max_time) / 1000))
        return (queries, Milliseconds(max_time))

    async def _fetch_latencies(
        self, client: Machine
    ) -> Optional[Tuple[Milliseconds, int]]:
        # just use the first viewer per machine
        cmd_result = await client.ssh.run(
            "journalctl --unit viewer@1 | grep -Eo 'Request took [0-9]+ms.'",
        )
        total_latency = 0
        total_count = 0
        for line in cmd_result.stdout.split("\n"):
            match = re.match(
                r"Request took ([0-9]+)ms.",
                line,
            )
            if not match:
                continue
            (time,) = match.groups()
            total_latency += int(time)
            total_count += 1
        if total_count == 0:
            return None
        return (Milliseconds(total_latency), total_count)

    async def _prepare_client(
        self,
        machine: Machine,
        count: int,
        etcd_env: Dict[str, Any],
        runtime: Optional[int],
    ):
        if self.hammer:
            nprocs = count
            if self.channels <= 10000:
                threads = 32
            elif self.channels <= 50000 or self.message_size <= 1000:
                threads = 16
            else:
                threads = 8
            if isinstance(self.protocol, SymmetricPub):
                threads //= 4
        else:
            threads = 20
            nprocs = count // threads
            assert count % threads == 0
        spectrum_config: Dict[str, Any] = {
            "SPECTRUM_TLS_CA": "/home/ubuntu/spectrum/data/ca.crt",
            "SPECTRUM_VIEWER_THREADS": threads,
            "SPECTRUM_LOG_LEVEL": "debug",
            **etcd_env,
        }
        if runtime:
            spectrum_config["SPECTRUM_MAX_JITTER_MILLIS"] = runtime
        await _install_spectrum_config(machine, spectrum_config)
        await machine.ssh.run(
            "sudo journalctl --rotate && sudo journalctl --vacuum-time=1s",
            check=True,
        )
        await machine.ssh.run(
            f"sudo systemctl start viewer@{{1..{nprocs}}}",
            check=True,
        )

    async def _execute_experiment(
        self,
        setting: Setting,
        etcd_env: Dict[str, Any],
    ) -> Result:
        spectrum_config: Dict[str, Any] = {
            "SPECTRUM_LOG_LEVEL": "trace",
            "SPECTRUM_DELAY_MS": 30000,
            **etcd_env,
        }
        await _install_spectrum_config(setting.publisher, spectrum_config)
        if self.hammer:
            await setting.publisher.ssh.run(
                "sudo systemctl start spectrum-publisher", check=True
            )
            timeout = EXPERIMENT_TIMEOUT - 10  # give some cleanup time
            if isinstance(self.protocol, SymmetricPub):
                timeout += 180
            await asyncio.sleep(timeout)
        else:
            await setting.publisher.ssh.run(
                "sudo systemctl start spectrum-publisher --wait", check=True
            )

        latencies = await asyncio.gather(*map(self._fetch_latencies, setting.clients))
        latencies = list(filter(None, latencies))
        if not latencies:
            raise RuntimeError("Couldn't get latencies")
        total_time = sum(map(itemgetter(0), latencies))
        total_requests = sum(map(itemgetter(1), latencies))
        mean_latency = int(total_time / total_requests)

        if self.hammer:
            results = await asyncio.gather(
                *map(self._fetch_timing, setting.all_workers)
            )
            results = list(filter(None, results))
            if not results:
                raise RuntimeError("No successful runs.")
            # We now have the total QPS per machine; let's aggregate.
            # Divide by self.groups so we don't double-count.
            total_qps = (
                sum(1000 * queries / time for (queries, time) in results) / self.groups
            )
            mean_time = mean(time for (queries, time) in results)
            return Result(
                experiment=self,
                time=Milliseconds(int(mean_time)),
                queries=(total_qps * mean_time / 1000),
                mean_latency=mean_latency,
            )
        else:
            result = await setting.publisher.ssh.run(
                "journalctl --unit spectrum-publisher "
                "    | grep -o 'Elapsed time: .*' "
                "    | sed 's/Elapsed time: \\(.*\\)ms/\\1/'",
                check=True,
            )
            time = int(result.stdout.strip())
            return Result(
                experiment=self,
                time=Milliseconds(time),
                queries=self.clients,
                mean_latency=mean_latency,
            )

    async def _inner_run(self, setting: Setting, spinner: Halo) -> Result:
        publisher = setting.publisher
        workers_east = setting.workers_east
        workers_west = setting.workers_west
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
        hammer_flag = "--hammer" if self.hammer else ""
        # can't use ssh.run(env=...) because the SSH server doesn't like it.
        await publisher.ssh.run(
            f"SPECTRUM_CONFIG_SERVER={etcd_url} "
            "/home/ubuntu/spectrum/setup"
            f"    {self.protocol.flag}"
            f"    {hammer_flag} "
            f"    --channels {self.channels}"
            f"    --clients {self.clients}"
            f"    --group-size {self.group_size}"
            f"    --groups {self.groups}"
            f"    --message-size {self.message_size}",
            check=True,
            timeout=30,
        )

        spinner.text = "[experiment] starting workers and clients"
        assert self.workers_per_machine <= MAX_WORKERS_PER_MACHINE
        tasks = []
        workers_by_region = cycle((iter(workers_east), iter(workers_west)))
        for (group, workers) in zip(range(self.groups), workers_by_region):
            worker_start_idx = 0
            for idx in range(self.worker_machines_per_group):
                worker = next(workers)
                leader = idx == 0 and not self.hammer
                task = _prepare_worker(
                    worker,
                    group + 1,
                    worker_start_idx,
                    self.workers_per_machine,
                    etcd_env,
                    leader,
                )
                worker_start_idx += self.workers_per_machine
                tasks.append(task)
        await asyncio.gather(*tasks)

        client_counts = distribute(self.clients, self.cpm)
        if self.expected_runtime or self.hammer:
            runtime = self.expected_runtime
        else:
            runtime = self.clients * 4
        await asyncio.gather(
            *[
                self._prepare_client(client, client_range, etcd_env, runtime)
                for client, client_range in zip(clients, client_counts)
            ]
        )

        spinner.text = "[experiment] running"
        timeout = EXPERIMENT_TIMEOUT + 30 if self.hammer else EXPERIMENT_LONG_TIMEOUT
        if isinstance(self.protocol, SymmetricPub):
            timeout += 180
        return await asyncio.wait_for(
            self._execute_experiment(setting, etcd_env),
            timeout=timeout,
        )

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        try:
            return await self._inner_run(setting, spinner)
        finally:
            spinner.text = "[experiment] shutting everything down"
            shutdowns = []
            all_workers = setting.workers_west + setting.workers_east
            for worker in all_workers:
                shutdowns.append(
                    worker.ssh.run(
                        "sudo systemctl stop 'spectrum-worker@*'", check=False
                    )
                )
                shutdowns.append(
                    worker.ssh.run("sudo systemctl stop spectrum-leader", check=False)
                )
            for client in setting.clients:
                shutdowns.append(
                    client.ssh.run("sudo systemctl stop 'viewer@*'", check=False)
                )
            shutdowns.append(
                setting.publisher.ssh.run(
                    "sudo systemctl stop spectrum-publisher", check=False
                )
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
