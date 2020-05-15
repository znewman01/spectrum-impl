import asyncio
import json
import operator
import sys

from contextlib import contextmanager, asynccontextmanager
from functools import reduce
from subprocess import check_call, check_output

import asyncssh


@contextmanager
def terraform(tf_vars, cleanup=False):
    tf_vars = reduce(operator.add, [["-var", f"{k}={v}"] for k, v in tf_vars.items()])
    check_call(["terraform", "apply", "-auto-approve"] + tf_vars)

    yield

    if cleanup:
        check_call(["terraform", "destroy", "-auto-approve"] + tf_vars)


@asynccontextmanager
async def infra(rebuild=False, cleanup=False):
    if rebuild:
        check_call(["packer", "build", "image.json"])

    with open("manifest.json") as f:
        data = json.load(f)
    (region, _, ami) = data["builds"][0]["artifact_id"].partition(":")

    with terraform({"ami": ami, "region": region}, cleanup=cleanup):
        hostname = (
            check_output(["terraform", "output", "hostname"]).decode("ascii").strip()
        )
        ssh_key = asyncssh.import_private_key(
            check_output(["terraform", "output", "private_key"]).decode("ascii")
        )
        yield await asyncssh.connect(
            hostname, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
        )


async def main(args):
    rebuild = "--rebuild" in args
    cleanup = "--cleanup" in args

    async with infra(rebuild, cleanup) as ssh:
        print(await ssh.run("whoami"))


if __name__ == "__main__":
    loop = asyncio.get_event_loop()
    loop.run_until_complete(main(sys.argv))
