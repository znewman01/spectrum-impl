# encoding: utf8
# pylint: disable=bad-continuation,ungrouped-imports,line-too-long
from __future__ import annotations

import asyncio
import contextlib
import traceback

from contextlib import asynccontextmanager, nullcontext
from dataclasses import dataclass
from typing import Optional, Set, List, Any, Dict, TypeVar, Awaitable


import asyncssh

from halo import Halo
from tenacity import wait_fixed, AsyncRetrying

from experiments import cloud, packer
from experiments.system import (
    BuildArgs,
    Args as SystemArgs,
    Setting,
    System,
    Experiment,
    Environment,
    experiments_by_environment,
    Machine,
)

MAX_ATTEMPTS = 5


@asynccontextmanager
async def _connect_ssh(hostname: str, *args, **kwargs):
    reraise_err = None
    async for attempt in AsyncRetrying(wait=wait_fixed(2)):
        with attempt:
            async with asyncssh.connect(hostname, *args, **kwargs) as conn:
                # SSH may be ready but really the system isn't until this file exists.
                await conn.run(
                    "test -f /var/lib/cloud/instance/boot-finished", check=True
                )
                try:
                    yield Machine(conn, cloud.Hostname(hostname))
                except BaseException as err:  # pylint: disable=broad-except
                    # Exceptions from "yield" have nothing to do with us.
                    # We reraise them below without retrying.
                    reraise_err = err
    if reraise_err is not None:
        raise reraise_err from None


# Pylint bug: https://github.com/PyCQA/pylint/issues/3401
K = TypeVar("K")  # pylint: disable=invalid-name
V = TypeVar("V")  # pylint: disable=invalid-name


async def _gather_dict(tasks: Dict[K, Awaitable[V]]) -> Dict[K, V]:
    async def do_it(key, coro):
        return key, await coro

    return dict(
        await asyncio.gather(*(do_it(key, coro) for key, coro in tasks.items()))
    )


@asynccontextmanager
async def infra(
    environment: Environment,
    system: System,
    force_rebuilt: Optional[Set[Any]],
    build_args: BuildArgs,
):
    Halo(f"[infrastructure] {environment}").stop_and_persist(symbol="•")

    packer_config = system.packer_config.from_args(build_args, environment)
    build = packer.ensure_ami_build(packer_config, force_rebuilt=force_rebuilt)

    with cloud.terraform(environment.make_tf_vars(build)) as data:
        ssh_key = asyncssh.import_private_key(data["private_key"])

        # The "stack" bit is so that we can have an async context manager that,
        # on exit, closes all of our SSH connections.
        async with contextlib.AsyncExitStack() as stack:
            conn_ctxs = {}
            for key, hostname in system.setting.to_machine_spec(data).items():
                conn_ctxs[key] = stack.enter_async_context(
                    _connect_ssh(
                        hostname,
                        known_hosts=None,
                        client_keys=[ssh_key],
                        username="ubuntu",
                    )
                )
            with Halo("[infrastructure] connecting (SSH) to all machines") as spinner:
                conns = await _gather_dict(conn_ctxs)
                spinner.succeed("[infrastructure] connected (SSH)")
            setting = system.setting.from_dict(conns)

            await setting.additional_setup()

            yield setting
        print()


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
                with open("error.log", "a") as log_file:
                    traceback.print_exc(file=log_file)
                msg = (
                    f"Error (attempt {attempt} of {MAX_ATTEMPTS}): "
                    f"{err!r} (traceback in [{log_file.name}])"
                )
                if attempt == MAX_ATTEMPTS:
                    spinner.fail(msg)
                else:
                    spinner.warn(msg)
            else:
                # experiment succeeded!
                spinner.succeed(f"[experiment] time: {result.time}ms")
                return result


@dataclass
class Args:
    packer: packer.Args
    cleanup: bool

    @staticmethod
    def add_args(parser):
        packer.Args.add_args(parser)
        parser.add_argument(
            "--cleanup", action="store_true", help="tear down all infrastructure after"
        )

    @classmethod
    def from_parsed(cls, parsed):
        return cls(packer=packer.Args.from_parsed(parsed), cleanup=parsed.cleanup)


async def run_experiments(
    all_experiments: List[Experiment],
    run_args: Args,
    subparser_args: SystemArgs,
    ctrl_c: asyncio.Event,  # general
):
    force_rebuilt = set() if run_args.packer.force_rebuild else None
    with cloud.cleanup(cloud.AWS_REGION) if run_args.cleanup else nullcontext():
        for environment, experiments in experiments_by_environment(all_experiments):
            async with infra(
                environment, subparser_args.system, force_rebuilt, subparser_args.build
            ) as setting:
                for experiment in experiments:
                    print()
                    Halo(f"{experiment}").stop_and_persist(symbol="•")
                    result = await retry_experiment(experiment, setting, ctrl_c)
                    yield result
