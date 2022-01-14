"""Interface for a system to run experiments on.

The overall flow of an experiment is as follows:

1. Build machine images (using Packer, if needed or requested)
2. Spin up infra (terraform)
3. run experiment(s)
4. spin down infra (`terraform destroy`, if cleanup=true)

Customizations needed:

1. packer: hopefully this is all always exactly one image

- packer.json
  - plus shell scripts etc.
- packer vars
  - some common
    - instance type?
    - region
  - some specific
    - sha  -- do we need to tar this up? when should it happen
    - src_archive
    - build profile

so probably:

- some function make_packer_vars(spectrum.PackerConfig)
  - might do real work
  - or require temp files -- context manager?

2. spin up infra

- TF
  - this is just `<experiment type>/main.tf`, plus other local file state
- TF variables
  - common
    - AMI
    - region
  - specific (come from Environment)
    - instance type
    - count for different types of machines
- mapping from TF output to Setting
  - ideally:
    - some f: environment x setup x tf data dict -> <iterable of machines to connect to>
    - then in infra(): connect to them all, give a dict or something
      - involves connecting to ssh a lot
    - map that dict to a Setting

3. running an experiment

lots, but i think it all goes in Experiment.run

4. none
"""
from __future__ import annotations

import io

from abc import ABC, abstractmethod
from dataclasses import dataclass
from itertools import groupby
from pathlib import Path
from typing import (
    List,
    NewType,
    Tuple,
    Any,
    ContextManager,
    Dict,
    Type,
    Protocol,
    Optional,
)

import asyncssh

from halo import Halo

from experiments.util import Hostname


@dataclass(frozen=True)
class Machine:
    ssh: asyncssh.SSHClientConnection
    hostname: Hostname
    _ssh_args: Dict[str, Any]


class Setting(ABC):
    """A handle to real-world resources needed to perform an experiment.

    For instance, these resources might be `Machines` organized by type.
    """

    @abstractmethod
    async def additional_setup(self):
        """

        Called after Setting created with valid SSH connections.
        """
        ...

    @staticmethod
    @abstractmethod
    def to_machine_spec(tf_data: Dict[str, Any]) -> Dict[Any, str]:
        ...

    @classmethod
    @abstractmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        ...


class _SupportsLessThan(Protocol):
    def __lt__(self, __other: Any) -> bool:
        ...


class Environment(_SupportsLessThan, ABC):
    """Description of what environment is required to do the experiment.

    Experiments with the same (`__eq__`) `Environment` should be able to share a
    `Setting`.
    """

    # TODO: remove "build" argument
    @abstractmethod
    def make_tf_vars(self, build: Any, build_args: BuildArgs) -> Dict[str, Any]:
        ...

    @staticmethod
    @abstractmethod
    def make_tf_cleanup_vars() -> Dict[str, Any]:
        ...


Milliseconds = NewType("Milliseconds", int)


@dataclass(frozen=True)
class Result:
    """The result of an experiment (with a pointer back to that Experiment)."""

    experiment: Experiment
    time: Milliseconds
    queries: int
    mean_latency: Optional[Milliseconds] = None

    @property
    def qps(self) -> float:
        return (self.queries / self.time) * 1000


class Experiment(ABC):
    """A specific experiment, with all parameters set.

    `run()` does the heavy lifting: given a `Setting` (handles to the necessary
    environment), actually *run* the experiment and return a result.

    Needs to be deserializable (`from_dict`) so that we can read in from JSON
    format.
    """

    @abstractmethod
    async def run(self, setting: Setting, spinner: Halo) -> Result:
        """Run (one trial of) the experiment and return its result."""
        ...

    @abstractmethod
    def to_environment(self) -> Environment:
        """Get a description of the environment this experiment runs in.

        Experiments that share an `Environment` will share a `Setting`.
        """
        ...

    @classmethod
    @abstractmethod
    def from_dict(cls, data) -> Experiment:
        ...


class PackerConfig(ABC):
    @abstractmethod
    def make_packer_args(self) -> ContextManager[Dict[str, str]]:
        """Yields the variables that should be passed on the command line to Packer.

        They will be formatted separately.

        Needs to be a context manager in case we need temp files.
        """
        ...

    @abstractmethod
    def matches(self, build: Dict[str, str]) -> bool:
        ...

    @classmethod
    @abstractmethod
    def from_args(cls, args: Any, environment: Environment) -> PackerConfig:
        ...


@dataclass
class System:
    """A system for which experiments should be run.

    Encapsulates all of the places where behaviors can vary."""

    environment: Type[Environment]
    experiment: Type[Experiment]
    setting: Type[Setting]
    packer_config: Type[PackerConfig]
    root_dir: Path


class BuildArgs(ABC):
    pass


class Args(ABC):
    system: System
    build: BuildArgs
    experiments_file: io.TextIOBase

    name: str
    doc: str

    @classmethod
    @abstractmethod
    def add_args(cls, parser):
        ...


def group_by_environment(
    experiments: List[Experiment],
) -> List[Tuple[Environment, List[Experiment]]]:
    experiments = sorted(experiments, key=lambda e: e.to_environment())
    by_environment = groupby(experiments, lambda e: e.to_environment())
    return [(k, list(v)) for k, v in by_environment]
