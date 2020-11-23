#!/usr/bin/env python
import argparse
import sys
import json
import os
import tempfile

from subprocess import check_output, check_call, CalledProcessError


def main():
    parser = argparse.ArgumentParser(description="SSH into a Terraform machine.")
    parser.add_argument(
        "--publisher",
        action="store_true",
        help="SSH into the publisher machine (default)",
    )
    parser.add_argument(
        "--client",
        type=int,
        nargs="?",
        const=0,
        help="SSH into some/(the nth) client machine",
    )
    parser.add_argument(
        "--worker",
        type=int,
        nargs="?",
        const=0,
        help="SSH into some/(the nth) worker machine",
    )
    args = parser.parse_args()

    cwd = os.path.dirname(__file__)  # terraform needs to be in *this* directory
    data = json.loads(check_output(["terraform", "output", "-json"], cwd=cwd))
    data = {k: v["value"] for k, v in data.items()}

    if args.client is not None:
        hostname = data["clients"][args.client]
    elif args.worker is not None:
        hostname = data["workers"][args.worker]
    else:
        hostname = data["publisher"]

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
                ]
            )
    except CalledProcessError as err:
        sys.exit(err.returncode)


if __name__ == "__main__":
    main()
