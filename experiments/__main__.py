# encoding: utf8
# Conflicts with black, isort
# pylint: disable=bad-continuation,ungrouped-imports,line-too-long
"""
Run experiments for Spectrum project.

Steps:

1. Build an appropriate base AMI.

   This step can take a very long time (compiling and packaging AMIs are both
   slow). We cache the AMI itself; use `--force-rebuild` to bust this cache.

2. Set up the AWS environment (using Terraform).

   This is quick (<20s to set everything up), though cleanup takes a while. See
   `main.tf` in each subdirectory for details.

3. Run experiments (retrying a few times if needed).

If running more than one experiment, they are grouped by AWS environment.

Requirements:

- Terraform (runnable as `terraform`)
- Packer (runnable as `packer`)
- Python 3.7
- Python dependencies: see `requirements.txt`
"""
from __future__ import annotations

import argparse
import asyncio
import json
import signal
import sys

from dataclasses import asdict

from experiments import spectrum
from experiments.util import stream_json, chdir
from experiments.run import run_experiments


def parse_args(args):
    description, _, epilog = __doc__.partition("\n\n")
    parser = argparse.ArgumentParser(
        description=description,
        epilog=epilog,
        formatter_class=argparse.RawTextHelpFormatter,
    )
    parser.add_argument(
        "--force-rebuild",
        action="store_true",
        help="rebuild the AMI (even if our source hasn't changed)",
    )
    parser.add_argument(
        "--cleanup", action="store_true", help="tear down all infrastructure after"
    )
    parser.add_argument("--commit", dest="commitish", help="commit(ish) to build for")
    parser.add_argument(
        "--output",
        default="results.json",
        type=argparse.FileType("w"),
        help="path for experiment results",
    )
    subparsers = parser.add_subparsers(required=True)
    for mod in [spectrum]:
        _, _, name = mod.__name__.rpartition(".")
        mod.add_args(
            subparsers.add_parser(
                name,
                help=mod.__doc__.lstrip(),
                formatter_class=argparse.RawTextHelpFormatter,
            )
        )
    return parser.parse_args(args[1:])


async def main(args):
    loop = asyncio.get_running_loop()
    ctrl_c = asyncio.Event()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, ctrl_c.set)

    all_experiments = list(
        map(args.experiment_cls.from_dict, json.load(args.experiments_file))
    )
    any_err = False
    with chdir(args.dir):
        try:
            with stream_json(args.output, close=True) as writer:
                async for result in run_experiments(
                    all_experiments,
                    args.force_rebuild,
                    args.commitish,
                    args.cleanup,
                    args.build,
                    ctrl_c,
                ):
                    if result is None:
                        any_err = True
                        continue
                    writer(asdict(result))
        except KeyboardInterrupt:
            pass
    if any_err:
        print("Error occurred")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main(parse_args(sys.argv)))
