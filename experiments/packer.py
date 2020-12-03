from __future__ import annotations

import json

from dataclasses import dataclass
from operator import attrgetter
from subprocess import check_call
from typing import Dict, Any, List, Optional, Set

from halo import Halo

from experiments.cloud import Region, AMI
from experiments import cloud, system


@dataclass
class Args:
    force_rebuild: bool

    @staticmethod
    def add_args(parser):
        parser.add_argument(
            "--force-rebuild",
            action="store_true",
            help="rebuild the AMI (even if our source hasn't changed)",
        )

    @classmethod
    def from_parsed(cls, parsed):
        return cls(force_rebuild=parsed.force_rebuild)


@dataclass(frozen=True)
class Build:
    timestamp: int
    region: Region
    ami: AMI
    custom_data: Dict[str, str]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Build:
        region, _, ami = data["artifact_id"].partition(":")
        return cls(
            timestamp=data["build_time"],
            region=region,
            ami=ami,
            custom_data=data["custom_data"],
        )


@dataclass(frozen=True)
class Manifest:
    # newest to oldest
    builds: List[Build]

    @classmethod
    def from_disk(cls, fname) -> Manifest:
        try:
            with open(fname) as manifest_file:
                data = json.load(manifest_file)
        except FileNotFoundError:
            return cls([])
        builds = list(map(Build.from_dict, data["builds"]))
        builds.sort(key=attrgetter("timestamp"), reverse=True)
        return cls(builds)

    def most_recent_matching(self, config: system.PackerConfig) -> Optional[Build]:
        for build in self.builds:
            if config.matches(build.custom_data):
                return build
        return None


def ensure_ami_build(
    config: system.PackerConfig, force_rebuilt: Optional[Set[system.PackerConfig]],
) -> Build:
    builds = Manifest.from_disk("manifest.json")
    build = builds.most_recent_matching(config)

    if force_rebuilt is not None and config not in force_rebuilt:
        force_rebuild = True
        force_rebuilt.add(config)
    else:
        force_rebuild = False

    if build is not None and not force_rebuild:
        return build

    with config.make_packer_args() as args:
        packer_vars = cloud.format_args(args)
        with open("packer.log", "w") as log_file:
            with Halo(
                f"[infrastructure] building AMI (output in [{log_file.name}])"
            ) as spinner:
                check_call(
                    ["packer", "build"] + packer_vars + ["packer.json"], stdout=log_file
                )
                spinner.succeed()

    builds = Manifest.from_disk("manifest.json")
    build = builds.most_recent_matching(config)
    if build is None:
        raise RuntimeError("Packer did not create the expected build.")
    return build
