"""Run Riposte experiments.

We use the `multiparty` branch of <https://bitbucket.org/henrycg/riposte>.
We create an Ubuntu AMI with Go and Riposte installed; this is sufficient to run
the Riposte experiments. However, Riposte is not configurable, so we need to
recompile for every set of parameters.

Our AWS environment:

- (many) client machines
- "leader" machine
- "server" machine
- "auditor" machine

A Spectrum experiment is parameterized by number of channels and message size.
Message size is easy in Riposte, but translating the number of channels into a
rectangular table (rows = width * height) is tricky.

Sec. 3.2 proposes a technique to give rows ~= 2.7 * channels; we use that
estimate. Then, we take the *minimum* of the results with each of two
width:height ratios: 1 (which fig. 4 suggests leads to greatest throughput) and
a ratio using the formula of sec. 4.3).

For each experiment, we:

1. Recompile the binaries.
2. Run `auditor`, `server`, and `leader` in that order.
3. Run the client, with the `-hammer` flag.
4. Kill after a while (AFAICT it just runs forever).
5. Parse the output of `auditor` to get the average QPS.

Step (5) is pretty generous -- we rely on the self-reported rate of the server
for each batch. This cuts out some idle time. But I don't think it changes the
results by much.


To debug:

    $ python ssh.py --client 4  # the 5th client
    $ python ssh.py --leader
    $ python ssh.py --server
    $ python ssh.py --auditor
"""
from __future__ import annotations
import argparse
import io

from dataclasses import dataclass
from experiments import system
from experiments.riposte import RIPOSTE


@dataclass
class Args(system.Args):
    experiments_file: io.TextIOBase
    build = None  # no build arguments

    system = RIPOSTE

    name = "riposte"
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
