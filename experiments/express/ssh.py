#!/usr/bin/env python
# pylint: disable=duplicate-code
import argparse
import sys
import json
import tempfile

from pathlib import Path
from subprocess import check_output, check_call, CalledProcessError


def main():
    parser = argparse.ArgumentParser(description="SSH into a Terraform machine.")
    parser.add_argument(
        "--servera",
        action="store_true",
        help="SSH into the leader machine (default)",
    )
    parser.add_argument(
        "--serverb",
        action="store_true",
        help="SSH into the other server machine",
    )
    parser.add_argument(
        "--client",
        type=int,
        nargs="?",
        const=0,
        help="SSH into some/(the nth) worker machine",
    )
    args = parser.parse_args()

    data = json.loads(
        check_output(
            ["terraform", "output", "-json"],
            cwd=Path(__file__).parent,
        )
    )
    data = {k: v["value"] for k, v in data.items()}

    if args.client is not None:
        hostname = data["clients"][args.client]
    elif args.servera:
        hostname = data["serverA"]
    else:
        hostname = data["serverB"]

    try:
        with tempfile.NamedTemporaryFile() as key_file:
            key_file.write(data["private_key"].encode("utf8"))
            key_file.flush()

            check_call(
                [
                    "ssh",
                    "-i",
                    key_file.name,
                    f"ubuntu@{hostname}",
                    "-o",
                    "StrictHostKeyChecking=no",
                ],
            )
    except CalledProcessError as err:
        sys.exit(err.returncode)


if __name__ == "__main__":
    main()
