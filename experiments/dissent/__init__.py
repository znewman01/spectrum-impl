from __future__ import annotations

import asyncio
import math
import re
import shlex
import socket

from contextlib import contextmanager
from dataclasses import dataclass
from functools import partial
from pathlib import Path
from typing import Any, Dict, Iterator, Union, Tuple, ClassVar, List, Optional

from halo import Halo

from experiments import system, packer
from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import DEFAULT_INSTANCE_TYPE, InstanceType, AWS_REGION
from experiments.util import Bytes

PORT = 6000


@dataclass
class Setting(system.Setting):
    clients: List[Machine]
    server0: Machine
    server1: Machine

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        for name in ("server0", "server1"):
            result[name] = tf_data[name]
        for idx, client in enumerate(tf_data["clients"]):
            result[("client", idx)] = client
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        server0 = None
        server1 = None
        clients = []
        for ident, machine in machines.items():
            if ident == "server0":
                server0 = machine
            elif ident == "server1":
                server1 = machine
            elif ident[0] == "client":
                clients.append(machine)
            else:
                raise ValueError(f"Invalid identifier [{ident}]")
        if server0 is None or server1 is None or not clients:
            raise ValueError(f"Missing machines; got {machines}")
        return cls(clients, server0, server1)

    async def additional_setup(self):
        pass

    def __iter__(self):
        return iter([self.server0, self.server1] + self.clients)


@dataclass(order=True, frozen=True)
class Environment(system.Environment):
    instance_type: InstanceType
    client_machine_count: int = 1

    def make_tf_vars(self, _build: Optional[packer.Build], _: Any) -> Dict[str, Any]:
        tf_vars = {
            "region": AWS_REGION,
            "instance_type": self.instance_type,
            "client_machine_count": self.client_machine_count,
        }
        return tf_vars

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "instance_type": DEFAULT_INSTANCE_TYPE,
            "client_machine_count": 0,
        }


async def _install(
    machine: Machine, template_path: Path, out_path: Path, env_vars: Dict[str, str]
):
    env_spec = " ".join([f"${key}" for key in env_vars])  # envsubst wants '$FOO $BAR'
    env = " ".join(
        [f"{key}={shlex.quote(str(value))}" for key, value in env_vars.items()]
    )
    await machine.ssh.run(
        f"{env} envsubst '{env_spec}' < {template_path} > {out_path}", check=True
    )


async def _run(connection, cmd: str, shutdown: asyncio.Event):
    async with connection.create_process(cmd) as process:
        await shutdown.wait()
        process.kill()
        return await process.stderr.read()


@dataclass(frozen=True)
class Experiment(system.Experiment):
    clients: int
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    channels: int = 1
    message_size: Bytes = Bytes(160)

    def to_environment(self) -> Environment:
        return Environment(instance_type=self.instance_type)

    @classmethod
    def from_dict(cls, data) -> Experiment:
        if "message_size" in data:
            data["message_size"] = Bytes(data["message_size"])
        return cls(**data)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        # clean up
        await asyncio.gather(
            *[m.ssh.run("pkill dissent || rm -f *.log", check=True) for m in setting]
        )

        # manage keys/IPs
        ls_proc = await setting.server0.ssh.run("ls keys/private", check=True)
        all_keys = sorted(filter(None, ls_proc.stdout.split("\n")))
        server0_key = all_keys.pop()
        server0_ip = (
            await setting.server0.ssh.run("ec2metadata --public-ip")
        ).stdout.strip()
        server1_key = all_keys.pop()
        server1_ip = (
            await setting.server1.ssh.run("ec2metadata --public-ip")
        ).stdout.strip()
        broadcaster_keys = [all_keys.pop() for _ in range(self.channels)]
        assert len(broadcaster_keys) == len(setting.clients)  # TODO: for now
        listener_keys = [all_keys.pop() for _ in range(self.clients - self.channels)]
        assert len(setting.clients) == 1
        # divide up broadcasters, listeners over client machines

        # set up configs
        common_vars = {
            "SERVER0_ADDR": server0_ip,
            "SERVER1_ADDR": server1_ip,
            "DISSENT_PORT": PORT,
            "SERVER_IDS": f'"{server0_key}","{server1_key}"',
        }
        server_conf = Path("config/server.conf")
        await _install(
            setting.server0,
            Path("config/server.conf.template"),
            server_conf,
            {"LOCAL_IDS": f'"{server0_key}"', **common_vars},
        )
        await _install(
            setting.server1,
            Path("config/server.conf.template"),
            server_conf,
            {"LOCAL_IDS": f'"{server1_key}"', **common_vars},
        )
        message_path = Path("message")
        for client in setting.clients:
            # TODO: handle >1 broadcaster per machine
            # - WEB_PORT
            # - multiple configs
            await _install(
                client,
                Path("config/broadcaster.conf.template"),
                Path("config/broadcaster.conf"),
                {
                    "LOCAL_IDS": f'"{broadcaster_keys.pop()}"',
                    "WEB_PORT": 8080,
                    **common_vars,
                },
            )
            # TODO: divide up listeners over machines
            listener_local_ids = ",".join(map('"{}"'.format, listener_keys))
            await _install(
                client,
                Path("config/client.conf.template"),
                Path("config/client.conf"),
                {
                    "LOCAL_IDS": listener_local_ids,
                    "NODES_PER_PROCESS": len(listener_keys),
                    **common_vars,
                },
            )
            cmd = (
                f"head -c {self.message_size} /dev/zero "
                f"| tr '\\0' 'a' "
                f"> {message_path}"
            )
            await client.ssh.run(
                cmd,
                check=True,
            )

        # run the dissent processes
        shutdown = asyncio.Event()
        dissent_bin = "./Dissent/dissent"
        server_cmd = f"{dissent_bin} config/server.conf"
        listener_cmd = f"{dissent_bin} config/client.conf"
        broadcaster_cmd = f"{dissent_bin} config/broadcaster.conf"
        proc_futures = (
            [
                asyncio.create_task(_run(setting.server0.ssh, server_cmd, shutdown)),
                asyncio.create_task(_run(setting.server1.ssh, server_cmd, shutdown)),
            ]
            + [
                asyncio.create_task(_run(c.ssh, listener_cmd, shutdown))
                for c in setting.clients
            ]
            + [
                asyncio.create_task(_run(c.ssh, broadcaster_cmd, shutdown))
                for c in setting.clients
            ]
        )
        all_procs = asyncio.gather(*proc_futures)
        try:
            await asyncio.sleep(
                20
            )  # TODO: wait until broadcaster log has "WaitingForServer"
            # and send messages!
            await asyncio.gather(
                *[
                    c.ssh.run(
                        "curl -X POST --data-binary @message localhost:8080/session/send",
                        check=True,
                    )
                    for c in setting.clients
                ]
            )
            WAIT_TIME = 300
            spinner.text = f"[experiment] run processes for {WAIT_TIME}s"
            await asyncio.sleep(WAIT_TIME)
            shutdown.set()
            spinner.text = "[experiment] waiting for processes to exit"
        finally:
            all_procs_output = await all_procs
        TIMEOUT = 5
        await asyncio.sleep(TIMEOUT)
        # TODO: harvest processes
        # TODO: check logs
        # return Result(experiment=self)


@dataclass(frozen=True)
class PackerConfig(system.PackerConfig):
    instance_type: InstanceType

    @contextmanager
    def make_packer_args(self) -> Iterator[Dict[str, str]]:
        yield {"instance_type": str(self.instance_type)}

    def matches(self, build: Dict[str, str]) -> bool:
        return self.instance_type == InstanceType(build["instance_type"])

    @classmethod
    def from_args(cls, args: Any, environment: Environment) -> PackerConfig:
        _ = args  # unused
        return PackerConfig(instance_type=environment.instance_type)


DISSENT = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path(__file__).parent,
)
