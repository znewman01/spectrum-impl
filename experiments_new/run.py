import asyncio
import contextlib
import json
import operator
import sys

from contextlib import contextmanager, asynccontextmanager
from dataclasses import dataclass
from functools import reduce
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory
from pathlib import Path

import asyncssh


@dataclass
class Machine:
    ssh: asyncssh.SSHClientConnection
    hostname: str


def _format_var_args(var_dict):
    return reduce(operator.add, [["-var", f"{k}={v}"] for k, v in var_dict.items()])


@contextmanager
def terraform(tf_vars, cleanup=False):
    tf_vars = _format_var_args(tf_vars)
    check_call(["terraform", "apply", "-auto-approve"] + tf_vars)

    data = json.loads(check_output(["terraform", "output", "-json"]))
    yield {k: v["value"] for k, v in data.items()}

    if cleanup:
        check_call(["terraform", "destroy", "-auto-approve"] + tf_vars)


def _get_last_build():
    with open("manifest.json") as f:
        data = json.load(f)
    return data["builds"][-1]  # most recent


def build_ami(force_rebuild=False):
    git_root = check_output(["git", "rev-parse", "--show-toplevel"]).strip()

    src_sha = (
        check_output(["git", "rev-list", "-1", "HEAD", "--", "spectrum"], cwd=git_root)
        .decode("ascii")
        .strip()
    )
    build = _get_last_build()
    build_sha = build["custom_data"].get("sha", None)
    if build_sha == src_sha and not force_rebuild:
        return build

    with TemporaryDirectory() as tmpdir:
        src_path = Path(tmpdir) / "spectrum-src.tar.gz"
        check_call(
            [
                "git",
                "archive",
                "--format",
                "tar.gz",
                "--output",
                str(src_path),
                "--prefix",
                "spectrum/",
                src_sha,
            ],
            cwd=git_root,
        )

        packer_vars = _format_var_args({"sha": src_sha, "src_archive": str(src_path)})
        check_call(["packer", "build"] + packer_vars + ["image.json"])

    return _get_last_build()


@asynccontextmanager
async def infra(force_rebuild=False, cleanup=False):
    build = build_ami(force_rebuild=force_rebuild)

    (region, _, ami) = build["artifact_id"].partition(":")
    instance_type = build["custom_data"]["instance_type"]

    tf_vars = {"ami": ami, "region": region, "instance_type": instance_type}
    with terraform(tf_vars, cleanup=cleanup) as data:
        publisher = data["publisher"]
        workers = data["workers"]
        clients = data["clients"]
        ssh_key = asyncssh.import_private_key(data["private_key"])

        conns = []
        conns.append(
            asyncssh.connect(
                publisher, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
            )
        )
        for worker in workers:
            conn = asyncssh.connect(
                worker, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
            )
            conns.append(conn)
        for client in clients:
            conn = asyncssh.connect(
                client, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
            )
            conns.append(conn)

        async with contextlib.AsyncExitStack() as stack:
            conns = [
                await stack.enter_async_context(ctx)
                for ctx in await asyncio.gather(*conns)
            ]
            publisher_machine = Machine(ssh=conns.pop(0), hostname=publisher)
            worker_machines = []
            for worker in workers:
                worker_machines.append(Machine(ssh=conns.pop(0), hostname=worker))
            client_machines = []
            for client in clients:
                client_machines.append(Machine(ssh=conns.pop(0), hostname=client))

            yield {
                "publisher": publisher_machine,
                "workers": worker_machines,
                "clients": client_machines,
            }


async def main(args):
    force_rebuild = "--force-rebuild" in args
    cleanup = "--cleanup" in args

    async with infra(force_rebuild, cleanup) as machines:
        publisher = machines.pop("publisher")
        workers = machines.pop("workers")
        clients = machines.pop("clients")

        actual_hostname = await publisher.ssh.run("hostname")
        print(f"{publisher.hostname}: {actual_hostname}")


if __name__ == "__main__":
    loop = asyncio.get_event_loop()
    loop.run_until_complete(main(sys.argv))
