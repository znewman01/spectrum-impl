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
import os
import signal
import sys

from contextlib import contextmanager, closing, nullcontext
from typing import Any, Dict, TextIO, Iterator, Callable, ContextManager
from pathlib import Path

from experiments import spectrum
from experiments.run import run_experiments
from experiments.spectrum import Experiment


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
        help="rebuild the AMI even if Spectrum source hasn't changed",
    )
    parser.add_argument(
        "--cleanup", action="store_true", help="tear down all infrastructure after"
    )
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


@contextmanager
def stream_json(
    f: TextIO, close: bool = False
) -> Iterator[Callable[[Dict[str, Any]], None]]:
    """Streams JSON objects to a file-like object.

    Hack around the fact that json.dump doesn't allow streaming.
    At the conclusion,

    If close is True, The file will be closed on exit.

    >>> with stream_json(open("test.json", "w")) as writer:
    ...   writer.write({"a": 1})
    ...   writer.write({"a": 1})
    >>> with open("test.json", "r") as f:
    ...   f.read() == '[\n{"a": 1},\n{"b": 2}\n]\n'
    True

    Args:
        f: file-like object (in str mode)
        close: if True, the f will be
    Yields:
        callable that writes its argument to f
    """
    closer: ContextManager = closing(f) if close else nullcontext()
    with closer:
        f.write("[\n")
        first = True

        def writer(data):
            nonlocal first
            if not first:
                f.write(",\n")
            first = False
            json.dump(data, f)
            f.flush()

        yield writer
        f.write("\n]\n")


@contextmanager
def chdir(path: Path):
    old_cwd = os.getcwd()
    try:
        os.chdir(path)
        yield
    finally:
        os.chdir(old_cwd)


async def main(args):
    loop = asyncio.get_running_loop()
    ctrl_c = asyncio.Event()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, ctrl_c.set)

    all_experiments = map(Experiment.from_dict, json.load(args.experiments_file))
    with chdir(args.dir):
        try:
            with stream_json(args.output, close=True) as writer:
                await run_experiments(
                    all_experiments,
                    writer,
                    args.force_rebuild,
                    args.cleanup,
                    args.build,
                    ctrl_c,
                )
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    asyncio.run(main(parse_args(sys.argv)))
