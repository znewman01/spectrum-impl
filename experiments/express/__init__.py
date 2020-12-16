from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Iterator, Union, Tuple

from halo import Halo

from experiments import system, packer
from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import InstanceType, AWS_REGION

EXPRESS = None


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


# client [serverAip:port] [serverBip:port] [numThreads] [rowDataSize] (optional)throughput
# serverA [serverBip:port] [numThreads] [numCores (set it to 0)] [numRows] [rowDataSize]
# serverB [numThreads] [numCores (set it to 0)] [numRows] [rowDataSize]
# numThreads = 2x cores on machine
# server A outputs

# serverA.go:209: Time Elapsed: 10.000124778s; number of writes: 644
@dataclass(frozen=True)
class Experiment(system.Experiment):
    instance_type: InstanceType = InstanceType("c5.9xlarge")
    # TODO

    def to_environment(self) -> Environment:
        return Environment(instance_type=self.instance_type)

    @classmethod
    def from_dict(cls, data) -> Experiment:
        return cls(**data)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        try:
            _client = setting.client
            _server_a = setting.server_a
            _server_b = setting.server_b

            spinner.text = "[experiment] setting up"

            # TODO: run experiment

        finally:
            pass
        return Result(self, Milliseconds(1000))


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


# pylint: disable=duplicate-code
EXPRESS = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path(__file__).parent,
)
# pylint: enable=duplicate-code
