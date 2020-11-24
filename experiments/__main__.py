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
import io
import json
import signal
import sys

from dataclasses import asdict, dataclass
from typing import List

from experiments import spectrum, Experiment
from experiments.util import stream_json, chdir
from experiments.run import run_experiments, Args as RunArgs


_SUBPARSER_MODS = [spectrum]


@dataclass
class Args:

    run: RunArgs
    subparser_args: spectrum.Args
    output: io.IOBase
    cleanup: bool

    @classmethod
    def add_args(cls, parser):
        RunArgs.add_args(parser)
        parser.add_argument(
            "--output",
            default="results.json",
            type=argparse.FileType("w"),
            help="path for experiment results",
        )
        subparsers = parser.add_subparsers(required=True)
        for mod in _SUBPARSER_MODS:
            _, _, name = mod.__name__.rpartition(".")
            subparser = subparsers.add_parser(
                name,
                help=mod.__doc__.lstrip(),
                formatter_class=argparse.RawTextHelpFormatter,
            )
            mod.Args.add_args(subparser)

    @classmethod
    def from_parsed(cls, parsed):
        return cls(
            run=RunArgs.from_parsed(parsed),
            subparser_args=parsed.arg_cls.from_parsed(parsed),
            output=parsed.output,
            cleanup=parsed.cleanup,
        )


def parse_args(args) -> Args:
    description, _, epilog = __doc__.partition("\n\n")
    parser = argparse.ArgumentParser(
        description=description,
        epilog=epilog,
        formatter_class=argparse.RawTextHelpFormatter,
    )
    Args.add_args(parser)
    return Args.from_parsed(parser.parse_args(args[1:]))


async def main(args: Args):
    loop = asyncio.get_running_loop()
    ctrl_c = asyncio.Event()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, ctrl_c.set)

    all_experiments: List[Experiment] = list(
        map(
            args.subparser_args.experiment_cls.from_dict,
            json.load(args.subparser_args.experiments_file),
        )
    )
    any_err = False
    with chdir(args.subparser_args.dir):
        try:
            with stream_json(args.output, close=True) as writer:
                async for result in run_experiments(
                    all_experiments, args.run, args.subparser_args, ctrl_c,
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
