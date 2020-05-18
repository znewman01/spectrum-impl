import asyncio
import contextlib
import json
import operator
import sys

from contextlib import contextmanager, asynccontextmanager
from dataclasses import dataclass
from functools import reduce
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory, NamedTemporaryFile
from pathlib import Path

import asyncssh

from tqdm import tqdm


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
            # TODO: this doesn't always work
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


async def _install_spectrum_conf(machine, spectrum_conf):
    spectrum_conf = "\n".join([f"{k}={v}" for k, v in spectrum_conf.items()])
    with NamedTemporaryFile() as tmp:
        tmp.write(spectrum_conf.encode("utf8"))
        tmp.flush()
        await asyncssh.scp(tmp.name, (machine.ssh, "/tmp/spectrum.conf"))
    await machine.ssh.run("sudo install -m 644 /tmp/spectrum.conf /etc/spectrum.conf")


async def _prepare_worker(machine, group, etcd_env):
    # TODO: WORKER_START_INDEX for multiple machines per group
    spectrum_conf = {
        "SPECTRUM_WORKER_GROUP": group,
        "SPECTRUM_LEADER_GROUP": group,
        "SPECTRUM_WORKER_START_INDEX": 0,
        **etcd_env,
    }
    await _install_spectrum_conf(machine, spectrum_conf)

    await machine.ssh.run("sudo systemctl start spectrum-worker@1")
    await machine.ssh.run("sudo systemctl start spectrum-leader")


async def _prepare_client(machine, etcd_env):
    await _install_spectrum_conf(machine, etcd_env)

    # TODO: fix client ranges
    await machine.ssh.run("sudo systemctl start viewer@{1..100}")


async def _execute_experiment(publisher, etcd_env):
    await _install_spectrum_conf(publisher, etcd_env)
    await publisher.ssh.run("sudo systemctl start spectrum-publisher --wait")

    result = await publisher.ssh.run(
        "journalctl --unit spectrum-publisher "
        "    | grep -o 'Elapsed time: .*' "
        "    | sed 's/Elapsed time: \\(.*\\)ms/\\1/'"
    )
    result = int(result.stdout.strip())

    # don't let this same output confuse us if we run on this machine again
    await publisher.ssh.run("sudo journalctl --rotate")
    await publisher.ssh.run("sudo journalctl --vacuum-time=1s")

    return result


async def run_experiment(publisher, workers, clients):
    # TODO: progress bars using tqdm
    # https://stackoverflow.com/questions/37901292/asyncio-aiohttp-progress-bar-with-tqdm
    tqdm.write("Starting etcd...")
    await publisher.ssh.run(
        "envsubst '$HOSTNAME' "
        '    < "$HOME/config/etcd.template" '
        "    | sudo tee /etc/default/etcd "
        "    > /dev/null"
    )
    await publisher.ssh.run("sudo systemctl start etcd")
    etcd_url = f"etcd://{publisher.hostname}:2379"
    etcd_env = {"SPECTRUM_CONFIG_SERVER": etcd_url}
    tqdm.write("etcd started.")

    try:
        tqdm.write("Setting up experiment...")
        # can't use ssh.run(env=...) because the SSH server doesn't like it.
        await publisher.ssh.run(
            f"SPECTRUM_CONFIG_SERVER={etcd_url} "
            "/home/ubuntu/spectrum/setup"
            "    --security 16"
            "    --channels 10"
            "    --clients 100"
            "    --group-size 1"
            "    --groups 2"
            "    --message-size 1024"
        )
        tqdm.write("Experiment set up.")

        tqdm.write("Preparing workers...")
        # TODO: fix for multiple machines per group etc.
        await asyncio.gather(
            *[
                _prepare_worker(worker, idx + 1, etcd_env)
                for idx, worker in enumerate(workers)
            ]
        )
        tqdm.write("Workers prepared.")

        tqdm.write("Preparing clients...")
        await asyncio.gather(*[_prepare_client(client, etcd_env) for client in clients])
        tqdm.write("Clients prepared.")

        tqdm.write("Executing experiment...")
        result = await _execute_experiment(publisher, etcd_env)
        tqdm.write("Experiment executed.")
        return result
    finally:
        tqdm.write("Shutting everything down...")
        shutdowns = []
        shutdowns.append(
            publisher.ssh.run(
                "ETCDCTL_API=3 etcdctl --endpoints localhost:2379 del --prefix ''"
            )
        )
        for worker in workers:
            shutdowns.append(worker.ssh.run("sudo systemctl stop spectrum-leader"))
            shutdowns.append(worker.ssh.run("sudo systemctl stop 'spectrum-worker@*'"))
        shutdowns.append(publisher.ssh.run("sudo systemctl stop spectrum-publisher"))
        await asyncio.gather(*shutdowns)
        tqdm.write("Everything shut down.")


async def main(args):
    force_rebuild = "--force-rebuild" in args
    cleanup = "--cleanup" in args

    async with infra(force_rebuild, cleanup) as machines:
        publisher = machines.pop("publisher")
        workers = machines.pop("workers")
        clients = machines.pop("clients")

        for _ in tqdm(range(5)):
            tqdm.write(
                "RESULT: " + str(await run_experiment(publisher, workers, clients))
            )


if __name__ == "__main__":
    try:
        asyncio.run(main(sys.argv))
    except KeyboardInterrupt:
        pass
