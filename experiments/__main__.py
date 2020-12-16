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

from argparse import RawTextHelpFormatter
from dataclasses import asdict, dataclass
from typing import List, Type

from experiments.spectrum.args import Args as SpectrumArgs
from experiments.express.args import Args as ExpressArgs

from experiments.system import Args as SystemArgs, System
from experiments.util import stream_json
from experiments.run import run_experiments, Args as RunArgs


_SYSTEM_ARGS: List[Type[SystemArgs]] = [SpectrumArgs, ExpressArgs]


@dataclass
class Args:

    run: RunArgs
    system_args: SystemArgs
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
        for args in _SYSTEM_ARGS:
            # pylint: disable=no-member
            # Pylint gets confused, but this is part of the ABC specification for SystemArgs.
            args.add_args(
                subparsers.add_parser(
                    args.name, help=args.doc, formatter_class=RawTextHelpFormatter,
                )
            )
            # pylint: enable=no-member

    @classmethod
    def from_parsed(cls, parsed):
        return cls(
            run=RunArgs.from_parsed(parsed),
            system_args=parsed.arg_cls.from_parsed(parsed),
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

    system: System = args.system_args.system
    experiments_json = json.load(args.system_args.experiments_file)
    experiments = list(map(system.experiment.from_dict, experiments_json))

    any_err = False
    try:
        with stream_json(args.output, close=True) as writer:
            async for result in run_experiments(
                experiments, args.run, args.system_args, ctrl_c,
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
