from __future__ import annotations

import json

from dataclasses import dataclass
from operator import attrgetter
from pathlib import Path
from subprocess import check_call
from tempfile import TemporaryDirectory
from typing import Dict, Any, List, Optional, Set

from halo import Halo

from experiments.cloud import Region, AWS_REGION, MachineType, AMI, SHA
from experiments import cloud
from experiments.spectrum import BuildProfile


@dataclass(frozen=True)
class Config:
    machine_type: MachineType
    sha: SHA
    profile: BuildProfile


@dataclass(frozen=True)
class Build:
    timestamp: int
    region: Region
    ami: AMI
    machine_type: MachineType
    sha: SHA
    profile: BuildProfile

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Build:
        region, _, ami = data["artifact_id"].partition(":")
        return cls(
            timestamp=data["build_time"],
            region=region,
            ami=ami,
            sha=data["custom_data"]["sha"],
            machine_type=data["custom_data"]["instance_type"],
            profile=data["custom_data"]["profile"],
        )

    def to_config(self) -> Config:
        return Config(self.machine_type, self.sha, self.profile)


@dataclass(frozen=True)
class Manifest:
    # newest to oldest
    builds: List[Build]

    @classmethod
    def from_disk(cls, fname) -> Manifest:
        try:
            with open(fname) as f:
                data = json.load(f)
        except FileNotFoundError:
            return cls([])
        builds = list(map(Build.from_dict, data["builds"]))
        builds.sort(key=attrgetter("timestamp"), reverse=True)
        return cls(builds)

    def most_recent_matching(self, build_config: Config) -> Optional[Build]:
        for build in self.builds:
            if build.to_config() == build_config:
                return build
        return None


def ensure_ami_build(
    build_config: Config, git_root: Path, force_rebuilt: Optional[Set[Config]]
) -> Build:
    builds = Manifest.from_disk("manifest.json")
    build = builds.most_recent_matching(build_config)

    if force_rebuilt is not None and build_config not in force_rebuilt:
        force_rebuild = True
        force_rebuilt.add(build_config)
    else:
        force_rebuild = False

    if build is not None and not force_rebuild:
        return build

    with TemporaryDirectory() as tmpdir:
        src_path = Path(tmpdir) / "spectrum-src.tar.gz"
        cmd = [
            "git",
            "archive",
            "--format",
            "tar.gz",
            "--output",
            str(src_path),
            "--prefix",
            "spectrum/",
            build_config.sha,
        ]
        check_call(cmd, cwd=git_root)

        packer_vars = cloud.format_args(
            {
                "sha": build_config.sha,
                "src_archive": str(src_path),
                "instance_type": build_config.machine_type,
                "region": AWS_REGION,
                "profile": build_config.profile,
            }
        )
        with open("packer.log", "w") as f:
            short_sha = build_config.sha[:7]
            with Halo(
                f"[infrastructure] building AMI (output in [packer.log]) for SHA: {short_sha}"
            ) as spinner:
                check_call(["packer", "build"] + packer_vars + ["image.json"], stdout=f)
                spinner.succeed()

    builds = Manifest.from_disk("manifest.json")
    build = builds.most_recent_matching(build_config)
    if build is None:
        raise RuntimeError("Packer did not create the expected build.")
    return build
