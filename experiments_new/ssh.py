#!/usr/bin/env python
import argparse
import sys
import json
import os
import tempfile

from subprocess import check_output, check_call, CalledProcessError

def main():
    parser = argparse.ArgumentParser(description="SSH into a Terraform machine.")
    parser.add_argument("--publisher", action="store_true", help="SSH into the publisher machine (default)")
    parser.add_argument("--client", type=int, nargs="?", const=0, help="SSH into some/(the nth) client machine")
    parser.add_argument("--worker", type=int, nargs="?", const=0, help="SSH into some/(the nth) worker machine")
    args = parser.parse_args()

    data = json.loads(check_output(["terraform", "output", "-json"]))
    data = {k: v["value"] for k, v in data.items()}

    if args.client is not None:
        hostname = data["clients"][args.client]
    elif args.worker is not None:
        hostname = data["workers"][args.worker]
    else:
        hostname = data["publisher"]

    try:
        with tempfile.NamedTemporaryFile() as f:
            f.write(data["private_key"].encode("utf8"))
            f.flush()

            check_call(["ssh", "-i", f.name, f"ubuntu@{hostname}", "-o", "StrictHostKeyChecking=no"])
    except CalledProcessError as err:
        sys.exit(err.status)

if __name__ == '__main__':
    main()
