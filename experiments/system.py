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
    - some function: environment x setup x tf data dict -> <iterable of machines to connect to>
    - then in infra(): connect to them all, give a dict or something
      - involves connecting to ssh a lot
    - map that dict to a Setting

3. lots, but i think it all goes in Experiment.run

4. none
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from itertools import groupby
from typing import List, NewType, Tuple, Any

from halo import Halo

from experiments.cloud import Machine


class Setting(ABC):
    @abstractmethod
    async def additional_setup(self):
        ...


class Environment(ABC):
    @abstractmethod
    def to_setup(self, machines: List[Machine]) -> Setting:
        ...

    def __lt__(self, other: Any) -> bool:
        ...


Milliseconds = NewType("Milliseconds", int)


@dataclass(frozen=True)
class Result:
    experiment: Experiment
    time: Milliseconds  # in millis; BREAKING CHANGE


class Experiment(ABC):
    @abstractmethod
    async def run(self, setting: Setting, spinner: Halo) -> Result:
        ...

    @abstractmethod
    def to_environment(self) -> Environment:
        ...

    @classmethod
    @abstractmethod
    def from_dict(cls, data) -> Experiment:
        pass


def experiments_by_environment(
    experiments: List[Experiment],
) -> List[Tuple[Environment, List[Experiment]]]:
    experiments = sorted(experiments, key=Experiment.to_environment)
    by_environment = groupby(experiments, Experiment.to_environment)
    return [(k, list(v)) for k, v in by_environment]
