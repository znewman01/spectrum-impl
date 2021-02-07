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
from experiments.cloud import DEFAULT_INSTANCE_TYPE, InstanceType, AWS_REGION
from experiments.util import Bytes


# the server outputs summary statistics every 10s; add one to avoid beating it
WAIT_TIME = 10 + 1


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
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    server_threads: int = 18  # Express: "1x or 2x the number of cores on the system"
    client_threads: int = 36  # "larger than the actual number of cores on the machine"
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
        client = setting.client
        server_a = setting.server_a
        server_b = setting.server_b

        spinner.text = "[experiment] starting processes"

        server_b_proc = server_b.ssh.create_process(
            f"cd Express/serverB && "
            f"./serverB {self.server_threads} "
            f"    0 "  # "cores" must be 0
            f"    {self.channels} "
            f"    {self.message_size}"
        )
        server_a_proc = server_a.ssh.create_process(
            f"cd Express/serverA && "
            f"./serverA {server_b.hostname}:4442 "
            f"    {self.server_threads} "
            f"    0 "  # "cores" must be 0
            f"    {self.channels} "
            f"    {self.message_size}"
        )
        client_proc = client.ssh.create_process(
            f"cd Express/client && "
            f"./client {server_a.hostname}:4443 {server_b.hostname}:4442 "
            f"    {self.client_threads} "
            f"    {self.message_size} "
            f"    throughput"
        )

        async with server_b_proc as server_b_proc:
            async with server_a_proc as server_a_proc:
                async with client_proc as client_proc:
                    spinner.text = f"[experiment] run processes for {WAIT_TIME}s"
                    await asyncio.sleep(WAIT_TIME)
                    server_a_proc.kill()
                    server_b_proc.kill()
                    client_proc.kill()
                    spinner.text = "[experiment] waiting for processes to exit"

        lines = (await server_a_proc.stderr.read()).split("\n")
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
                for line in (await server_b_proc.stderr.read()).split("\n"):
                    log_file.write(line + "\n")
                log_file.write("\n\nCLIENT\n")
                for line in (await client_proc.stderr.read()).split("\n"):
                    log_file.write(line + "\n")
            raise ValueError(f"No lines matched; output in {log_path}")
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
