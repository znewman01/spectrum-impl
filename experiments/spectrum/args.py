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
import io

from dataclasses import dataclass
from pathlib import Path
from subprocess import check_output

from experiments import system
from experiments.spectrum import BuildProfile, SPECTRUM
from experiments.cloud import SHA


def _get_git_root() -> Path:
    cmd = ["git", "rev-parse", "--show-toplevel"]
    return Path(check_output(cmd).decode("ascii").strip())


def _get_last_sha(git_root: Path) -> SHA:
    # Last sha at which the spectrum/ directory was modified
    # (i.e., changes to the actual code, not infrastructure)
    # This is mostly a hack to make changing the infrastructure bearable because
    # Packer builds take a long time.
    cmd = ["git", "rev-list", "-1", "HEAD", "--", "spectrum"]
    return SHA(check_output(cmd, cwd=git_root).decode("ascii").strip())


def _sha_for_commitish(git_root: Path, commitish: str) -> SHA:
    cmd = ["git", "rev-parse", commitish]
    return SHA(check_output(cmd, cwd=git_root).decode("ascii").strip())


@dataclass
class BuildArgs(system.BuildArgs):
    profile: BuildProfile
    sha: SHA
    git_root: Path

    @staticmethod
    def add_args(parser):
        parser.add_argument(
            "--build",
            choices=["debug", "release"],
            default="debug",
            type=BuildProfile,
            help="build profile for compilation",
        )
        parser.add_argument(
            "--commit", dest="commitish", help="commit(ish) to build for"
        )

    @classmethod
    def from_parsed(cls, parsed):
        profile = BuildProfile(parsed.build)

        git_root = _get_git_root()
        if parsed.commitish:
            sha = SHA(_sha_for_commitish(git_root, parsed.commit))
        else:
            sha = SHA(_get_last_sha(git_root))

        return cls(profile=profile, sha=sha, git_root=git_root)


@dataclass
class Args(system.Args):
    build: BuildArgs
    experiments_file: io.TextIOBase

    system = SPECTRUM

    name = "spectrum"
    doc = __doc__.lstrip()

    @classmethod
    def add_args(cls, parser):
        BuildArgs.add_args(parser)
        parser.add_argument(
            "experiments_file",
            metavar="EXPERIMENTS_FILE",
            type=argparse.FileType("r"),
            help="""\
JSON input.

For example:

    {"clients": 100, "channels": 10, "message_size": 1024}

Also configurable:

- `instance_type`: AWS instance type (same for all machines).
- `clients_per_machine`
- `workers_per_machine`: *processes* to run on each machine
- `worker_machines_per_group`: worker *machines* in each group
- `protocol`: the protocol to run; can be:
  - `{"Symmetric": {"security": 16}}` (16-byte prime, 2 groups)
  - `{"Insecure": {"parties": 3}}` (3 groups)
  - `{"SeedHomomorphic": {"parties": 3}}` (3 groups, default security)
""",
        )
        parser.set_defaults(arg_cls=cls)

    @classmethod
    def from_parsed(cls, parsed) -> Args:
        return cls(
            build=BuildArgs.from_parsed(parsed),
            experiments_file=parsed.experiments_file,
        )
