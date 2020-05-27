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
import argparse
import json

from pathlib import Path
from typing import NewType

import experiments

BuildProfile = NewType("BuildProfile", str)

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
    parser.set_defaults(main=main)
    parser.set_defaults(dir=Path(__file__).parent)


async def main(args, writer, ctrl_c):
    all_experiments = map(experiments.Experiment.from_dict, json.load(args.experiments_file))
    await experiments.run_experiments(
        all_experiments,
        writer,
        args.force_rebuild,
        args.cleanup,
        args.build,
        ctrl_c,
       )
