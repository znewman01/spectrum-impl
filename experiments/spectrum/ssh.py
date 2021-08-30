#!/usr/bin/env python
import argparse
import sys
import json
import os
import tempfile

from subprocess import check_output, check_call, CalledProcessError


def main(argv):
    parser = argparse.ArgumentParser(
        prog=argv[0], description="SSH into a Terraform machine."
    )
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
    if "--" in argv:
        idx = argv.index("--")
        argv, extra = argv[:idx], argv[idx + 1 :]
    else:
        extra = []
    args = parser.parse_args(argv[1:])

    cwd = os.path.dirname(__file__)  # terraform needs to be in *this* directory
    data = json.loads(check_output(["terraform", "output", "-json"], cwd=cwd))
    data = {k: v["value"] for k, v in data.items()}

    if args.client is not None:
        hostname = data["clients"][args.client]
    elif args.worker is not None:
        workers = data["workers_east"] + data["workers_west"]
        hostname = workers[args.worker]
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
                + extra
            )
    except CalledProcessError as err:
        sys.exit(err.returncode)


if __name__ == "__main__":
    main(sys.argv)
