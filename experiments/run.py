# encoding: utf8
# pylint: disable=bad-continuation,ungrouped-imports,line-too-long
from __future__ import annotations

import asyncio
import contextlib
import traceback

from contextlib import asynccontextmanager, nullcontext
from dataclasses import asdict
from subprocess import check_output
from typing import Optional, Set, List, Callable
from pathlib import Path

import asyncssh

from halo import Halo
from tenacity import wait_fixed, AsyncRetrying

from experiments import cloud, packer
from experiments import Environment, Setting, experiments_by_environment
from experiments.cloud import Machine, SHA
from experiments.spectrum import Experiment

MAX_ATTEMPTS = 5


def _get_git_root() -> Path:
    cmd = ["git", "rev-parse", "--show-toplevel"]
    return Path(check_output(cmd).decode("ascii").strip())


def _get_last_sha(git_root: Path) -> SHA:
    # Last sha at which the spectrum/ directory was modified
    # (i.e., changes to the actual code, not infrastructure)
    # This is mostly a hack to make changing the infrastructure bearable because
    # Packer builds take a long time.
    cmd = ["git", "rev-list", "-1", "HEAD", "--", "spectrum"]
    return SHA(check_output(cmd, cwd=git_root).decode("ascii").strip())


def _sha_for_commitish(git_root: Path, commitish: str) -> SHA:
    cmd = ["git", "rev-parse", commitish]
    return SHA(check_output(cmd, cwd=git_root).decode("ascii").strip())


@asynccontextmanager
async def _connect_ssh(*args, **kwargs):
    reraise_err = None
    async for attempt in AsyncRetrying(wait=wait_fixed(2)):
        with attempt:
            async with asyncssh.connect(*args, **kwargs) as conn:
                # SSH may be ready but really the system isn't until this file exists.
                await conn.run(
                    "test -f /var/lib/cloud/instance/boot-finished", check=True
                )
                try:
                    yield conn
                except BaseException as err:  # pylint: disable=broad-except
                    # Exceptions from "yield" have nothing to do with us.
                    # We reraise them below without retrying.
                    reraise_err = err
    if reraise_err is not None:
        raise reraise_err from None


@asynccontextmanager
async def infra(
    environment: Environment,
    force_rebuilt: Optional[Set[packer.Config]],
    build_profile: str,
    commitish: Optional[str]
):
    Halo(f"[infrastructure] {environment}").stop_and_persist(symbol="•")

    git_root = _get_git_root()
    sha = _sha_for_commitish(git_root, commitish) if commitish else _get_last_sha(git_root)

    build_config = packer.Config(
        instance_type=environment.instance_type, sha=sha, profile=build_profile
    )
    build = packer.ensure_ami_build(build_config, git_root, force_rebuilt=force_rebuilt)

    tf_vars = {
        "ami": build.ami,
        "region": build.region,
        "instance_type": build.instance_type,
        "client_machine_count": environment.client_machines,
        "worker_machine_count": environment.worker_machines,
    }
    with cloud.terraform(tf_vars) as data:
        publisher = data["publisher"]
        workers = data["workers"]
        clients = data["clients"]
        ssh_key = asyncssh.import_private_key(data["private_key"])

        conn_ctxs = []
        conn_ctxs.append(
            _connect_ssh(
                publisher, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
            )
        )
        for worker in workers:
            conn_ctxs.append(
                _connect_ssh(
                    worker, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
                )
            )
        for client in clients:
            conn_ctxs.append(
                _connect_ssh(
                    client, known_hosts=None, client_keys=[ssh_key], username="ubuntu"
                )
            )

        async with contextlib.AsyncExitStack() as stack:
            with Halo("[infrastructure] connecting (SSH) to all machines") as spinner:
                conns = await asyncio.gather(*map(stack.enter_async_context, conn_ctxs))
                spinner.succeed("[infrastructure] connected (SSH)")
            hostnames = [publisher] + workers + clients
            machines = [
                Machine(ssh, hostname) for ssh, hostname in zip(conns, hostnames)
            ]
            setup = environment.to_setup(machines)

            await setup.additional_setup()

            yield setup
        print()


def check_ssh(ssh_result):
    if ssh_result.exit_status != 0:
        raise Exception("bad")
    return ssh_result


async def retry_experiment(
    experiment: Experiment, setting: Setting, ctrl_c: asyncio.Event
):
    interrupted = False
    for attempt in range(1, MAX_ATTEMPTS + 1):
        with Halo() as spinner:
            experiment_task = asyncio.create_task(experiment.run(setting, spinner))
            ctrl_c.clear()
            ctrl_c_task = asyncio.create_task(ctrl_c.wait())

            await asyncio.wait(
                [experiment_task, ctrl_c_task], return_when=asyncio.FIRST_COMPLETED
            )
            if ctrl_c.is_set():
                experiment_task.cancel()
                try:
                    await experiment_task
                except asyncio.CancelledError:
                    pass

                # On the first ^C for a given trial, just continue.
                if not interrupted:
                    spinner.info(
                        "Got Ctrl+C; retrying (do it again to quit everything)."
                    )
                    interrupted = True
                    continue

                # On the second, quit everything.
                spinner.info("Got ^C multiple times; exiting.")
                raise KeyboardInterrupt
            try:
                result = await experiment_task
            except Exception as err:  # pylint: disable=broad-except
                with open("error.log", "a") as f:
                    traceback.print_exc(file=f)
                msg = f"Error (attempt {attempt} of {MAX_ATTEMPTS}): {err!r} (traceback in [error.log])"
                if attempt == MAX_ATTEMPTS:
                    spinner.fail(msg)
                else:
                    spinner.warn(msg)
            else:
                # experiment succeeded!
                spinner.succeed(f"[experiment] time: {result.time}ms")
                return result


async def run_experiments(
    all_experiments: List[Experiment],
    writer: Callable[[str], None],
    force_rebuild: bool,
    commitish: Optional[str],
    cleanup: bool,
    build_profile: str,
    ctrl_c: asyncio.Event,
):
    force_rebuilt = set() if force_rebuild else None
    any_err = False
    with cloud.cleanup(cloud.AWS_REGION) if cleanup else nullcontext():
        for environment, experiments in experiments_by_environment(all_experiments):
            async with infra(environment, force_rebuilt, build_profile, commitish) as setting:
                for experiment in experiments:
                    print()
                    Halo(f"{experiment}").stop_and_persist(symbol="•")
                    result = await retry_experiment(experiment, setting, ctrl_c)
                    if result is None:
                        any_err = True
                        continue
                    writer(asdict(result))
    return not any_err
