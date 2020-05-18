import asyncio
import json
import operator
import sys

from contextlib import contextmanager, asynccontextmanager
from functools import reduce
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory
from pathlib import Path

import asyncssh


def _format_var_args(var_dict):
    return reduce(operator.add, [["-var", f"{k}={v}"] for k, v in var_dict.items()])


@contextmanager
def terraform(tf_vars, cleanup=False):
    tf_vars = _format_var_args(tf_vars)
    check_call(["terraform", "apply", "-auto-approve"] + tf_vars)

    yield

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
    with terraform(tf_vars, cleanup=cleanup):
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
    force_rebuild = "--force-rebuild" in args
    cleanup = "--cleanup" in args

    async with infra(force_rebuild, cleanup) as ssh:
        print(await ssh.run("whoami"))


if __name__ == "__main__":
    loop = asyncio.get_event_loop()
    loop.run_until_complete(main(sys.argv))
