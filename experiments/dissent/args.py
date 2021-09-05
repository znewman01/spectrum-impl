"""Run Dissent experiments.

Use the source from <https://github.com/dedis/Dissent> (current commit as of
2021-09-05).

We create an Ubuntu AMI with the Dissent binaries. Our AWS environment:

- (many) client machines
- "server0" machine
- "server1" machine

A Spectrum experiment is parameterized by number of channels and message size.

For each experiment, we:

1. Set up config files (server0/server1 keys are baked into the AMI; the clients
   get generated locally)
2. Run `server0` and `server1`.
3. Parse?

To debug:

    $ python ssh.py --client 4  # the 5th client
    $ python ssh.py --server0
    $ python ssh.py --server1
"""
from __future__ import annotations
import argparse
import io

from dataclasses import dataclass
from experiments import system
from experiments.dissent import DISSENT


@dataclass
class Args(system.Args):
    experiments_file: io.TextIOBase
    build = None  # no build arguments

    system = DISSENT

    name = "dissent"
    doc = __doc__.lstrip()

    @classmethod
    def add_args(cls, parser):
        # TODO: fix up help
        parser.add_argument(
            "experiments_file",
            metavar="EXPERIMENTS_FILE",
            type=argparse.FileType("r"),
            nargs="?",
            help="""\
JSON input.

For example:

    {"channels": 10, "message_size": 1024}

Also configurable:

- `instance_type`: AWS instance type (same for all machines).
""",
        )
        parser.set_defaults(arg_cls=cls)

    @classmethod
    def from_parsed(cls, parsed) -> Args:
        return cls(
            experiments_file=parsed.experiments_file,
        )
