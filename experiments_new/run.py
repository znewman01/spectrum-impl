from __future__ import annotations

import argparse
import asyncio
import contextlib
import itertools
import json
import math
import operator
import sys

from contextlib import contextmanager, asynccontextmanager
from dataclasses import dataclass, field
from enum import Enum
from functools import reduce
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory, NamedTemporaryFile
from typing import Dict, Union, List
from pathlib import Path

import asyncssh

from tqdm import tqdm
from tqdm.contrib import DummyTqdmFile
from halo import Halo


# To use Halo + tqdm together
@contextmanager
def std_out_err_redirect_tqdm():
    old = sys.stdout, sys.stderr
    try:
        sys.stdout, sys.stderr = map(DummyTqdmFile, old)
        yield old[0]
    finally:
        sys.stdout, sys.stderr = old


@dataclass(frozen=True)
class Machine:
    ssh: asyncssh.SSHClientConnection
    hostname: str


@dataclass(frozen=True)
class Symmetric:
    security: int = field(default=16)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Symmetric:
        return cls(**data)


@dataclass(frozen=True)
class Insecure:
    parties: int = field(default=2)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Insecure:
        return cls(**data)


@dataclass(frozen=True)
class SeedHomomorphic:
    parties: int

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> SeedHomomorphic:
        return cls(**data)


def protocol_from_dict(
    data: Dict[str, Any]
) -> Union[Symmetric, Insecure, SeedHomomorphic]:
    keys = set(data.keys())
    assert len(keys) == 1
    key = keys.pop()
    if key == "Symmetric":
        return Symmetric.from_dict(data[key])
    if key == "Insecure":
        return Insecure.from_dict(data[key])
    if key == "SeedHomomorphci":
        return SeedHomomorphic.from_dict(data[key])
    raise ValueError(f"Invalid protocol {data}")


@dataclass(order=True, frozen=True)
class Environment:
    machine_type: str
    client_machines: int
    worker_machines: int
    workers_per_machine: int  # TODO: move out of environment (just have some max allowed value)


@dataclass(frozen=True)
class Experiment:
    # TODO: should just be one machine type?
    clients: int
    channels: int
    message_size: int
    machine_types: Dict[str, str] = field(default_factory=lambda: {"all": "c5.9xlarge"})
    clients_per_machine: int = field(default=250)
    workers_per_machine: int = field(default=1)
    worker_machines_per_group: int = field(default=1)
    protocol: Union[Symmetric, Insecure, SeedHomomorphic] = field(default=Symmetric())

    def groups(self) -> int:
        if isinstance(self.protocol, Symmetric):
            return 2
        elif isinstance(self.protocol, Insecure):
            return self.protocol.parties
        elif isinstance(self.protocol, SeedHomomorphic):
            return self.protocol.parties
        else:
            raise TypeError(
                f"Invalid protocol {self.protocol}. "
                "Expected one of Symmetric, Insecure, SeedHomomorphic"
            )

    def group_size(self) -> int:
        return self.workers_per_machine * self.worker_machines_per_group

    @property
    def machine_type(self) -> str:
        all_machine_types = set(self.machine_types.values())
        assert (
            len(all_machine_types) == 1
        ), f"Expected all identical machine types. Got {self.machine_types}"
        return all_machine_types.pop()

    def to_environment(self) -> Environment:
        client_machines = math.ceil(self.clients / self.clients_per_machine)
        worker_machines = self.worker_machines_per_group * self.groups()
        return Environment(
            machine_type=self.machine_type,
            worker_machines=worker_machines,
            client_machines=client_machines,
            workers_per_machine=self.workers_per_machine,
        )

    @classmethod
    def from_dict(cls, data) -> Experiment:
        protocol = data.pop("protocol", None)
        if protocol is not None:
            data["protocol"] = protocol_from_dict(protocol)
        return cls(**data)


def experiments_by_environment(
    experiments: List[Experiment]
) -> Dict[Environment, List[Experiment]]:
    experiments = sorted(experiments, key=Experiment.to_environment)
    by_environment = itertools.groupby(experiments, Experiment.to_environment)
    return {k: list(v) for k, v in by_environment}


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
async def infra(
    environment: Environment, force_rebuild: bool = False, cleanup: bool = False
):
    assert environment.worker_machines == 2
    assert environment.workers_per_machine == 1
    assert environment.client_machines == 1
    assert environment.machine_type == "c5.9xlarge"

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


async def run_experiment(
    experiment: Experiment,
    publisher: Machine,
    workers: List[Machine],
    clients: List[Machine],
):
    assert experiment.clients == 100
    assert experiment.channels == 10
    assert experiment.message_size == 1024
    assert experiment.clients <= experiment.clients_per_machine
    assert experiment.workers_per_machine == 1
    assert experiment.worker_machines_per_group == 1
    assert experiment.protocol == Symmetric(security=16)
    assert experiment.groups() == 2
    assert experiment.group_size() == 1

    # TODO: progress bars using tqdm
    # https://stackoverflow.com/questions/37901292/asyncio-aiohttp-progress-bar-with-tqdm
    with Halo(text="Starting etcd"):
        await publisher.ssh.run(
            "envsubst '$HOSTNAME' "
            '    < "$HOME/config/etcd.template" '
            "    | sudo tee /etc/default/etcd "
            "    > /dev/null"
        )
        await publisher.ssh.run("sudo systemctl start etcd")
    etcd_url = f"etcd://{publisher.hostname}:2379"
    etcd_env = {"SPECTRUM_CONFIG_SERVER": etcd_url}

    try:
        with Halo(text="Preparing experiment"):
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

            # TODO: fix for multiple machines per group etc.
            await asyncio.gather(
                *[
                    _prepare_worker(worker, idx + 1, etcd_env)
                    for idx, worker in enumerate(workers)
                ]
            )

            await asyncio.gather(
                *[_prepare_client(client, etcd_env) for client in clients]
            )

        with Halo(text="Running experiment") as spinner:
            result = await _execute_experiment(publisher, etcd_env)
            spinner.succeed(f"Experiment time: {result}ms")
            return result
    finally:
        with Halo(text="Shutting everything down"):
            shutdowns = []
            shutdowns.append(
                publisher.ssh.run(
                    "ETCDCTL_API=3 etcdctl --endpoints localhost:2379 del --prefix ''"
                )
            )
            for worker in workers:
                shutdowns.append(worker.ssh.run("sudo systemctl stop spectrum-leader"))
                shutdowns.append(
                    worker.ssh.run("sudo systemctl stop 'spectrum-worker@*'")
                )
            shutdowns.append(
                publisher.ssh.run("sudo systemctl stop spectrum-publisher")
            )
            await asyncio.gather(*shutdowns)


async def main(args):
    parser = argparse.ArgumentParser()
    parser.add_argument("--force-rebuild", action="store_true")
    parser.add_argument("--no-cleanup", action="store_false", dest="cleanup")
    parser.add_argument(
        "experiments_file", metavar="EXPERIMENTS_FILE", type=argparse.FileType("r")
    )
    args = parser.parse_args(args[1:])

    all_experiments = map(Experiment.from_dict, json.load(args.experiments_file))
    for environment, experiments in experiments_by_environment(all_experiments).items():
        async with infra(environment, args.force_rebuild, args.cleanup) as machines:
            publisher = machines.pop("publisher")
            workers = machines.pop("workers")
            clients = machines.pop("clients")

            for experiment in experiments:
                with std_out_err_redirect_tqdm():
                    await run_experiment(experiment, publisher, workers, clients)


if __name__ == "__main__":
    try:
        asyncio.run(main(sys.argv))
    except KeyboardInterrupt:
        pass
