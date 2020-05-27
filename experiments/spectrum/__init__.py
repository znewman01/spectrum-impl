"""
Run Spectrum experiments.

We use the Git commit at which the `spectrum/` source directory was last
modified to create an AMI for running experiments. This AMI has the Spectrum
binaries and all dependencies for running them, along with etcd. See
`image.json`, `install.sh`, `compile.sh`, and the contents of `config/` for
details on this image. In addition to the AMI itself, we cache the compiled
Spectrum binary.

Our AWS environment:

- 1 publisher machine (this also runs etcd)
- many worker machines (`worker_machines_per_group * num_groups`)
- as many client machines as necessary (`ceil(num_clients / clients_per_machine)`)

For each experiment, we:

1. Clear out prior state (etcd config, old logs).
2. Run the `setup` binary to put an experiment setup in etcd.
3. Start workers, leaders, and clients (pointing them at etcd).
4. Run the publisher, which will initiate the experiment.
5. Parse the publisher logs to get the time from start to finish.


To debug:

    $ python ssh.py  # SSH into publisher machine
    $ python ssh.py --worker # SSH into some worker
    $ python ssh.py --client 2 # SSH into the worker 2 (0-indexed)
"""
from __future__ import annotations

import argparse
import json
import math

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from pathlib import Path
from typing import NewType, Dict, Any, Optional, Type

from experiments.foo import Environment
from experiments.cloud import MachineType

BuildProfile = NewType("BuildProfile", str)
Bytes = NewType("Bytes", int)

EXPERIMENT_JSON_HELP = """\
JSON input.

For example:

    {"clients": 100, "channels": 10, "message_size": 1024}

Also configurable:

- `machine_type`: AWS instance type (same for all machines).
- `clients_per_machine`
- `workers_per_machine`: *processes* to run on each machine
- `worker_machines_per_group`: worker *machines* in each group
- `protocol`: the protocol to run; can be:
  - `{"Symmetric": {"security": 16}}` (16-byte prime, 2 groups)
  - `{"Insecure": {"parties": 3}}` (3 groups)
  - `{"SeedHomomorphic": {"parties": 3}}` (3 groups, default security)
"""


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
        return f"--security-multi-key 16"

    @classmethod
    def _from_dict(cls, data: Dict[str, Any]) -> SeedHomomorphic:
        return cls(**data)


@dataclass(frozen=True)
class Experiment:
    # TODO: should just be one machine type?
    clients: int
    channels: int
    message_size: Bytes
    machine_type: MachineType = field(default=MachineType("c5.9xlarge"))
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
            machine_type=self.machine_type,
            worker_machines=worker_machines,
            client_machines=client_machines,
        )

    @classmethod
    def from_dict(cls, data) -> Experiment:
        protocol = data.pop("protocol", None)
        if protocol is not None:
            data["protocol"] = Protocol.from_dict(protocol)
        return cls(**data)


def add_args(parser):
    parser.add_argument(
        "--build",
        choices=["debug", "release"],
        default="debug",
        type=BuildProfile,
        help="build profile for compilation",
    )
    parser.add_argument(
        "experiments_file",
        metavar="EXPERIMENTS_FILE",
        type=argparse.FileType("r"),
        help=EXPERIMENT_JSON_HELP,
    )
    parser.set_defaults(dir=Path(__file__).parent)


async def main(args, writer, ctrl_c):
