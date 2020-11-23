from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from itertools import groupby
from typing import List, NewType, Tuple, Any

from halo import Halo

from experiments.cloud import Machine

Milliseconds = NewType("Milliseconds", int)


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
