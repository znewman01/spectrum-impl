# encoding: utf8
# pylint: disable=bad-continuation,ungrouped-imports,line-too-long
from __future__ import annotations

import asyncio
import contextlib
import traceback

from contextlib import asynccontextmanager, nullcontext
from dataclasses import dataclass
from typing import Optional, Set, List, Any, AsyncIterator


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
    group_by_environment,
    Machine,
    Result,
)
from experiments.util import Hostname, gather_dict

MAX_ATTEMPTS = 5


@asynccontextmanager
async def _connect_ssh(hostname: Hostname, *args, **kwargs) -> AsyncIterator[Machine]:
    """Connect to the given host, retrying if necessary.

    A pretty simple wrapper around asyncssh.connect, with a couple of changes:

    - waits for /var/lib/cloud/instance/boot-finished to exist (AWS Ubuntu has
      this when the machine is ready)
    - yields a Machine instead of just connections: a nice wrapper of the
      connection with a hostname
    - retries until the machine is ready
    """

    reraise_err = None
    async for attempt in AsyncRetrying(wait=wait_fixed(2)):
        with attempt:
            async with asyncssh.connect(hostname, *args, **kwargs) as conn:
                # SSH may be ready but really the system isn't until this file exists.
                await conn.run(
                    "test -f /var/lib/cloud/instance/boot-finished", check=True
                )
                try:
                    yield Machine(conn, Hostname(hostname))
                except BaseException as err:  # pylint: disable=broad-except
                    # Exceptions from "yield" have nothing to do with us.
                    # We reraise them below without retrying.
                    reraise_err = err
    if reraise_err is not None:
        raise reraise_err from None


@asynccontextmanager
async def deployed(
    environment: Environment,
    system: System,
    force_rebuilt: Optional[Set[Any]],
    build_args: BuildArgs,
) -> AsyncIterator[Setting]:
    """Yields a Setting (handle to populated environment).

    This might require a few steps:

    1. create a Packer image (if it's missing or forced)
    2. `terraform apply`
    3. connecting (SSH) to the deployed machines
    4. performing additional setup (depending on the system)

    The "if forced" bit is a little tricky. We keep a (mutable) set of the
    configurations that have been rebuilt, since we want to rebuild at most once
    per execution. If that argument is None, we don't force rebuilding.
    """
    Halo(f"[infrastructure] {environment}").stop_and_persist(symbol="•")

    packer_config = system.packer_config.from_args(build_args, environment)
    build = packer.ensure_ami_build(packer_config, force_rebuilt, system.root_dir)

    with cloud.terraform(environment.make_tf_vars(build), system.root_dir) as data:
        ssh_key = asyncssh.import_private_key(data["private_key"])

        # The "stack" bit is so that we can have an async context manager that,
        # on exit, closes all of our SSH connections.
        async with contextlib.AsyncExitStack() as stack:
            conn_ctxs = {}
            for key, hostname in system.setting.to_machine_spec(data).items():
                conn_ctxs[key] = stack.enter_async_context(
                    _connect_ssh(
                        Hostname(hostname),
                        known_hosts=None,
                        client_keys=[ssh_key],
                        username="ubuntu",
                    )
                )
            with Halo("[infrastructure] connecting (SSH) to all machines") as spinner:
                conns = await gather_dict(conn_ctxs)
                spinner.succeed("[infrastructure] connected (SSH)")
            setting = system.setting.from_dict(conns)

            await setting.additional_setup()

            yield setting
        print()


async def retry_experiment(
    experiment: Experiment, setting: Setting, ctrl_c: asyncio.Event
) -> Optional[Result]:
    """Run the experiment up to MAX_ATTEMPTS times."""
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
                spinner.succeed(
                    f"[experiment] {result.queries} queries in {result.time}ms => {result.qps} qps"
                )
                return result
    return None


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
    args: Args,
    system_args: SystemArgs,
    ctrl_c: asyncio.Event,
):
    """Run all experiments.

    This groups the experiments by their environment so that we can reuse a
    given environment.

    We clean up the environment at the end if requested (by args.cleanup).
    """
    system = system_args.system
    # a mutable set indicates that we should rebuild everything (but at most one
    # time per execution!)
    force_rebuilt = set() if args.packer.force_rebuild else None

    cleanup = cloud.cleanup(system) if args.cleanup else nullcontext()
    with cleanup:
        for env, env_experiments in group_by_environment(all_experiments):
            build_args = system_args.build
            async with deployed(env, system, force_rebuilt, build_args) as setting:
                for experiment in env_experiments:
                    print()
                    Halo(f"{experiment}").stop_and_persist(symbol="•")
                    result = await retry_experiment(experiment, setting, ctrl_c)
                    yield result
