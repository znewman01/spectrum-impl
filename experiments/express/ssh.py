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
    args = parser.parse_args()

    data = json.loads(check_output(["terraform", "output", "-json"]))
    data = {k: v["value"] for k, v in data.items()}

    if True:
        hostname = data["publisher"]

    try:
        with tempfile.NamedTemporaryFile() as f:
            f.write(data["private_key"].encode("utf8"))
            f.flush()

            check_call(["ssh", "-i", f.name, f"ubuntu@{hostname}", "-o", "StrictHostKeyChecking=no"])
    except CalledProcessError as err:
        sys.exit(err.returncode)

if __name__ == '__main__':
    main()
