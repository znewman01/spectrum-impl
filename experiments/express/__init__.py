from __future__ import annotations

import asyncio
import re

from contextlib import contextmanager
from dataclasses import dataclass
from functools import partial
from pathlib import Path
from typing import Any, Dict, Iterator, Union, Tuple, ClassVar

from halo import Halo

from experiments import system, packer
from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import InstanceType, AWS_REGION
from experiments.util import Bytes


@dataclass
class Setting(system.Setting):
    client: Machine
    server_a: Machine
    server_b: Machine

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        for name in ("client", "serverA", "serverB"):
            result[name] = tf_data[name]
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        return cls(
            client=machines["client"],
            server_a=machines["serverA"],
            server_b=machines["serverB"],
        )

    async def additional_setup(self):
        pass


@dataclass(order=True, frozen=True)
class Environment(system.Environment):
    instance_type: InstanceType

    def make_tf_vars(self, build: packer.Build) -> Dict[str, Any]:
        return {
            "ami": build.ami,
            "region": build.region,
            "instance_type": self.instance_type,
        }

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "ami": "null",
            "instance_type": "null",
        }


@dataclass(frozen=True)
class Experiment(system.Experiment):
    instance_type: InstanceType = InstanceType("c5.9xlarge")
    server_threads: int = 18  # Express docs: "1x or 2x the number of cores on the system"
    client_threads: int = 36  # Express docs: "set numThreads larger than the actual number of cores on the machine"
    channels: int = 1
    message_size: Bytes = Bytes(1000)

    CORES: ClassVar[int] = 0  # must be 0

    # the server outputs summary statistics every 10s; add one to avoid beating it
    WAIT_TIME: ClassVar[int] = 10 + 1

    RESULT_RE: ClassVar[str] = (
        r"serverA.go:209: "
        r"Time Elapsed: (?P<time>.*)s; "
        r"number of writes: (?P<queries>.*)"
    )

    def to_environment(self) -> Environment:
        return Environment(instance_type=self.instance_type)

    @classmethod
    def from_dict(cls, data) -> Experiment:
        if "message_size" in data:
            data["message_size"] = Bytes(data["message_size"])
        return cls(**data)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        client = setting.client
        server_a = setting.server_a
        server_b = setting.server_b

        spinner.text = "[experiment] starting processes"

        cmd_b = f"cd Express/serverB && ./serverB {self.server_threads} {self.CORES} {self.channels} {self.message_size}"
        cmd_a = f"cd Express/serverA && ./serverA {server_b.hostname}:4442 {self.server_threads} {self.CORES} {self.channels} {self.message_size}"
        cmd_client = f"cd Express/client && ./client {server_a.hostname}:4443 {server_b.hostname}:4442 {self.client_threads} {self.message_size} throughput"
        server_b_proc = server_b.ssh.create_process(cmd_b)
        server_a_proc = server_a.ssh.create_process(cmd_a)
        client_proc = client.ssh.create_process(cmd_client)

        async with server_b_proc, server_a_proc, client_proc:
            spinner.text = f"[experiment] letting processes run for {self.WAIT_TIME}s"
            await asyncio.sleep(self.WAIT_TIME)
            server_a_proc.kill()
            spinner.text = "[experiment] waiting for processes to exit"

        lines = (await server_a_proc.stderr.read()).split("\n")
        matching = list(filter(None, map(partial(re.match, self.RESULT_RE), lines)))
        return Result(
            experiment=self,
            time=Milliseconds(int(float(matching[-1].group("time")) * 1000)),
            queries=int(matching[-1].group("queries")),
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
