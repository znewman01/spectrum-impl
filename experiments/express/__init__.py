from __future__ import annotations

import asyncio
import re

from contextlib import contextmanager
from dataclasses import dataclass
from functools import partial
from pathlib import Path
from typing import Any, Dict, Iterator, Union, Tuple, ClassVar, Optional

from halo import Halo

from experiments import system, packer
from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import DEFAULT_INSTANCE_TYPE, InstanceType, AWS_REGION
from experiments.util import Bytes


# the server outputs summary statistics every 10s; add one to avoid beating it
WAIT_TIME = 30 + 3


@dataclass
class Setting(system.Setting):
    clients: List[Machine]
    server_a: Machine
    server_b: Machine

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        for name in ("serverA", "serverB"):
            result[name] = tf_data[name]
        for idx, client in enumerate(tf_data["clients"]):
            result[("client", idx)] = client
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        server_a = None
        server_b = None
        clients = []
        for ident, machine in machines.items():
            if ident == "serverA":
                server_a = machine
            elif ident == "serverB":
                server_b = machine
            elif ident[0] == "client":
                clients.append(machine)
            else:
                raise ValueError(f"Invalid identifier [{ident}]")
        return cls(clients=clients, server_a=server_a, server_b=server_b)

    async def additional_setup(self):
        pass


@dataclass(order=True, frozen=True)
class Environment(system.Environment):
    instance_type: InstanceType
    # Express doesn't like multiple client machines! It stops processing as soon
    # as the second one tries to connect.
    client_machine_count: int = 1

    def make_tf_vars(self, build: Optional[packer.Build], _: Any) -> Dict[str, Any]:
        tf_vars = {
            "instance_type": self.instance_type,
            "region": AWS_REGION,
            "client_machine_count": self.client_machine_count,
        }
        return tf_vars

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "instance_type": DEFAULT_INSTANCE_TYPE,
            "client_machine_count": 0,
        }


async def _run(connection, cmd: str, shutdown: asyncio.Event):
    async with connection.create_process(cmd) as process:
        await shutdown.wait()
        process.kill()
        return await process.stderr.read()


@dataclass(frozen=True)
class Experiment(system.Experiment):
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    server_threads: int = 16  # Express: "1x or 2x the number of cores on the system"
    client_threads: int = 64  # "larger than the actual number of cores on the machine"
    channels: int = 1
    message_size: Bytes = Bytes(1000)

    def to_environment(self) -> Environment:
        return Environment(instance_type=self.instance_type)

    @classmethod
    def from_dict(cls, data) -> Experiment:
        if "message_size" in data:
            data["message_size"] = Bytes(data["message_size"])
        return cls(**data)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        clients = setting.clients
        server_a = setting.server_a
        server_b = setting.server_b

        spinner.text = "[experiment] starting processes"

        server_b_cmd = (
            f"cd Express/serverB && "
            f"./serverB {self.server_threads} "
            f"    0 "  # "cores" must be 0
            f"    {self.channels} "
            f"    {self.message_size}"
        )
        server_a_cmd = (
            f"sleep 0.5 && "
            f"cd Express/serverA && "
            f"./serverA {server_b.hostname}:4442 "
            f"    {self.server_threads} "
            f"    0 "  # "cores" must be 0
            f"    {self.channels} "
            f"    {self.message_size}"
        )
        client_cmd = (
            f"sleep 1 && "
            f"ec2metadata --public-ip && "
            f"cd Express/client && "
            f"./client {server_a.hostname}:4443 {server_b.hostname}:4442 "
            f"    {self.client_threads} "
            f"    {self.message_size} "
            f"    throughput"
        )

        shutdown = asyncio.Event()
        proc_futures = [
            asyncio.create_task(_run(server_b.ssh, server_b_cmd, shutdown)),
            asyncio.create_task(_run(server_a.ssh, server_a_cmd, shutdown)),
        ] + [
            asyncio.create_task(_run(c.ssh, client_cmd, shutdown))
            for (idx, c) in enumerate(clients)
        ]
        all_procs = asyncio.gather(*proc_futures)
        spinner.text = f"[experiment] run processes for {WAIT_TIME}s"
        await asyncio.sleep(WAIT_TIME)
        shutdown.set()
        spinner.text = "[experiment] waiting for processes to exit"
        all_procs_output = await all_procs

        lines = (all_procs_output[1]).split("\n")
        result_regex = (
            r"serverA.go:209: "
            r"Time Elapsed: (?P<time>.*)s; "
            r"number of writes: (?P<queries>.*)"
        )
        matching = list(filter(None, map(partial(re.match, result_regex), lines)))
        if not matching:
            log_path = Path("express.log")
            with open(log_path, "w") as log_file:
                log_file.write("SERVER A\n")
                for line in lines:
                    log_file.write(line + "\n")
                log_file.write("\n\nSERVER B\n")
                for line in (all_procs_output[0]).split("\n"):
                    log_file.write(line + "\n")
                log_file.write("\n\nCLIENT\n")
                for line in (all_procs_output[-1]).split("\n"):
                    log_file.write(line + "\n")
            if not matching:
                raise ValueError(f"No lines matched; output in {log_path}")
        results = [(float(m.group("time")), int(m.group("queries"))) for m in matching]
        best = max(results, key=lambda x: x[1] / x[0])
        return Result(
            experiment=self,
            time=Milliseconds(int(best[0] * 1000)),
            queries=int(best[1]),
        )


@dataclass(frozen=True)
class PackerConfig(system.PackerConfig):
    instance_type: InstanceType

    @contextmanager
    def make_packer_args(self) -> Iterator[Dict[str, str]]:
        yield {"instance_type": str(self.instance_type)}

    def matches(self, build: Dict[str, str]) -> bool:
        return self.instance_type == InstanceType(build["instance_type"])

    @classmethod
    def from_args(cls, args: Any, environment: Environment) -> PackerConfig:
        _ = args  # unused
        return PackerConfig(instance_type=environment.instance_type)


EXPRESS = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path(__file__).parent,
)
