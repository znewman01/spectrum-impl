"""Run Express experiments.

We use the HEAD of the Git repo: `https://github.com/SabaEskandarian/Express`.
We create an Ubuntu AMI with Go and Express installed; this is sufficient to run
the Express experiments.

Our AWS environment:

- client machine
- "serverA" machine
- "serverB" machine

For each experiment, we:

1. Run `serverA`, `serverB`, and `client` (in throughput mode) in that order.
2. Kill after a while (AFAICT it just runs forever).
3. Parse the output of `serverA` to get the average QPS.

To debug:

    $ python ssh.py client
    $ python ssh.py serverA
    $ python ssh.py serverB
"""
from __future__ import annotations
import argparse
import io

from dataclasses import dataclass
from experiments import system
from experiments.express import EXPRESS


@dataclass
class Args(system.Args):
    experiments_file: io.TextIOBase
    build = None  # no build arguments

    system = EXPRESS

    name = "express"
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
