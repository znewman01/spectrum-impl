from __future__ import annotations

import asyncio
import asyncssh
import asyncssh.misc
import math

from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime
from functools import partial
from pathlib import Path
from typing import Any, Dict, Iterator, Union, Tuple, ClassVar, List, Optional
from hashlib import sha256
from tempfile import NamedTemporaryFile

from halo import Halo

from experiments import system, packer
from experiments.system import Milliseconds, Result, Machine
from experiments.cloud import DEFAULT_INSTANCE_TYPE, InstanceType, AWS_REGION
from experiments.util import Bytes

PORT = 6000
WAIT_TIME_SHORT = 120
WAIT_TIME_LONG = 600
LISTENERS_PER_PROCESS = 50
CLIENTS_PER_MACHINE = 200

_SERVER0_ID = "QUTDkL8mYss2gBw-E2fx1GGAh2w="
_SERVER1_ID = "h8m9jFrEqu4bOcUBxYilGQMsYXE="


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


async def _install_file(machine: Machine, contents: str, remote: Path):
    tmp_fname = sha256(contents.encode("utf8")).hexdigest()[:6]
    tmp_path = f"/tmp/{tmp_fname}"
    with NamedTemporaryFile() as tmp:
        tmp.write(contents.encode("utf8"))
        tmp.flush()
        await asyncssh.scp(tmp.name, (machine.ssh, tmp_path))
    await machine.ssh.run(f"cp {tmp_path} {remote}", check=True)


async def _run(machine: Machine, cmd: str, shutdown: asyncio.Event):
    async with asyncssh.connect(str(machine.hostname), **machine._ssh_args) as conn:
        async with conn.create_process(cmd) as process:
            await shutdown.wait()
            process.kill()


async def _get_ip(machine: Machine) -> str:
    cmd = "ec2metadata --public-ip"
    proc = await machine.ssh.run(cmd)
    return proc.stdout.strip()


def _distribute(balls: int, bins: int) -> List[int]:
    counts = [balls // bins] * bins
    remainder = balls % bins
    for i in range(remainder):
        counts[i] += 1
    assert sum(counts) == balls
    assert len(counts) == bins
    return counts


@dataclass(frozen=True)
class Experiment(system.Experiment):
    clients: int
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    channels: int = 1
    message_size: Bytes = Bytes(160)
    blame: bool = False

    def to_environment(self) -> Environment:
        client_machine_count = math.ceil(self.clients / CLIENTS_PER_MACHINE)
        return Environment(
            instance_type=self.instance_type, client_machine_count=client_machine_count
        )

    @classmethod
    def from_dict(cls, data) -> Experiment:
        if "message_size" in data:
            data["message_size"] = Bytes(data["message_size"])
        return cls(**data)

    async def _cleanup(self, setting: Setting):
        await asyncio.gather(
            *[
                m.ssh.run(
                    "pkill dissent || pkill curl || rm -f *.{log,conf}", check=True
                )
                for m in setting
            ]
        )

    async def _install_keys(self, machines: List[Machine]):
        futs = []
        keys_dir = "Dissent/conf/local"
        cp_cmds = []
        for key in (_SERVER0_ID, _SERVER1_ID):
            cp_cmds.append(f"cp -f {keys_dir}/public/{key}.pub keys/public/")
            cp_cmds.append(f"cp -f {keys_dir}/private/{key} keys/private/")
        for machine in machines:
            for cp_cmd in cp_cmds:
                futs.append(machine.ssh.run(cp_cmd, check=True))
        await asyncio.gather(*futs)

    async def _sort_keys(self, machine: Machine) -> Dict[str, Union[str, List[str]]]:
        # Bizarrely, Dissent just silently fails when our servers have keys that
        # we generated. It's fine for our clients. Rather than figure out the
        # bug, just use the demo keys it ships with.
        ls_proc = await machine.ssh.run("ls keys/private", check=True)
        key_dict = {}
        all_keys = ls_proc.stdout.split("\n")
        all_keys = [k for k in all_keys if k and k not in (_SERVER0_ID, _SERVER1_ID)]
        all_keys.sort()
        key_dict["server0"] = _SERVER0_ID
        key_dict["server1"] = _SERVER1_ID
        key_dict["broadcasters"] = [all_keys.pop() for _ in range(self.channels)]
        key_dict["listeners"] = [
            all_keys.pop() for _ in range(self.clients - self.channels)
        ]
        return key_dict

    async def _sort_ips(
        self, server0: Machine, server1: Machine, clients: List[Machine]
    ):
        return {
            "server0": await _get_ip(server0),
            "server1": await _get_ip(server1),
            "clients": await asyncio.gather(*map(_get_ip, clients)),
        }

    _LOCAL_CONFIG_DIR = Path(__file__).parent / "config"
    _SERVER_CONF_TEMPLATE_PATH = _LOCAL_CONFIG_DIR / "server.conf"
    _SERVER_CONF_PATH = Path("server.conf")
    _BROADCASTER_CONF_TEMPLATE_PATH = _LOCAL_CONFIG_DIR / "broadcaster.conf"
    _BROADCASTER_CONF_PATH = "broadcaster{}.conf"
    _CLIENT_CONF_PATH = "client{}.conf"
    _CLIENT_CONF_TEMPLATE_PATH = _LOCAL_CONFIG_DIR / "client.conf"

    async def _install_configs(
        self, setting: Setting, ips: Dict[str, str], keys: Dict[str, str]
    ):
        # TODO: move to signature?
        server0 = setting.server0
        server1 = setting.server1
        clients = setting.clients
        round_type = "neff/csdcnet" if self.blame else "null/csdcnet"
        common_vars = {
            "server0_addr": ips["server0"],
            "server1_addr": ips["server1"],
            "dissent_port": PORT,
            "server_ids": f'"{keys["server0"]}","{keys["server1"]}"',
            "round_type": round_type,
        }
        server_conf_template = self._SERVER_CONF_TEMPLATE_PATH.read_text()
        server0_conf = server_conf_template.format(
            **{"local_id": keys["server0"], **common_vars}
        )
        await _install_file(server0, server0_conf, self._SERVER_CONF_PATH)
        server1_conf = server_conf_template.format(
            **{"local_id": keys["server1"], **common_vars}
        )
        await _install_file(server1, server1_conf, self._SERVER_CONF_PATH)
        message_path = Path("message")
        broadcaster_conf_template = self._BROADCASTER_CONF_TEMPLATE_PATH.read_text()
        client_conf_template = self._CLIENT_CONF_TEMPLATE_PATH.read_text()
        broadcaster_keys = list(keys["broadcasters"])
        listener_keys = list(keys["listeners"])
        listener_counts = _distribute(len(listener_keys), len(clients))
        broadcaster_counts = _distribute(self.channels, len(clients))
        counts = []  # for each client: (num_broadcasters, num_listener_processes)
        for client, listener_count, broadcaster_count in zip(
            clients, listener_counts, broadcaster_counts
        ):
            for b_idx in range(broadcaster_count):
                broadcaster_vars = {
                    "local_id": broadcaster_keys.pop(),
                    "port": 6100 + b_idx,
                    "web_port": 8850 + b_idx,
                    "entry_port": 8950 + b_idx,
                    "idx": b_idx,
                    **common_vars,
                }
                broadcaster_conf = broadcaster_conf_template.format(**broadcaster_vars)
                await _install_file(
                    client, broadcaster_conf, self._BROADCASTER_CONF_PATH.format(b_idx)
                )
            listener_local_ids = listener_keys[:listener_count]
            listener_keys = listener_keys[listener_count:]
            listener_counts = _distribute(
                listener_count, math.ceil(listener_count / LISTENERS_PER_PROCESS)
            )
            for l_idx, lcount in enumerate(listener_counts):
                listener_local_process_ids = [
                    listener_local_ids.pop() for _ in range(lcount)
                ]
                listener_local_id_str = ",".join(
                    map('"{}"'.format, listener_local_process_ids)
                )
                client_vars = {
                    "local_ids": listener_local_id_str,
                    "nodes_per_process": len(listener_local_process_ids),
                    "port": 6200 + l_idx,
                    "web_port": 8800 + l_idx,
                    "entry_port": 8900 + l_idx,
                    "idx": l_idx,
                    **common_vars,
                }
                client_conf = client_conf_template.format(**client_vars)
                await _install_file(
                    client, client_conf, self._CLIENT_CONF_PATH.format(l_idx)
                )
            # make a message_size-byte dummy file
            cmd = (
                f"head -c {self.message_size} /dev/zero "
                f"| tr '\\0' 'a' "
                f"> {message_path}"
            )
            await client.ssh.run(cmd, check=True)
            counts.append((broadcaster_count, len(listener_counts)))
        return counts

    def _run_dissent(
        self, setting: Setting, counts: List[Tuple[int, int]], shutdown: asyncio.Event
    ):
        server0 = setting.server0
        server1 = setting.server1
        clients = setting.clients
        dissent_bin = "./Dissent/dissent"

        proc_futures = []
        server_cmd = f"{dissent_bin} {self._SERVER_CONF_PATH}"
        proc_futures.extend(
            [
                asyncio.create_task(_run(server0, server_cmd, shutdown)),
                asyncio.create_task(_run(server1, server_cmd, shutdown)),
            ]
        )
        for client, (broadcasters, listeners) in zip(clients, counts):
            broadcaster_confs = map(
                self._BROADCASTER_CONF_PATH.format, range(broadcasters)
            )
            broadcaster_cmds = [
                f"{dissent_bin} {c} > /tmp/bcast.log 2>&1" for c in broadcaster_confs
            ]
            proc_futures.extend(
                [
                    asyncio.create_task(_run(client, b, shutdown))
                    for b in broadcaster_cmds
                ]
            )
            client_confs = map(self._CLIENT_CONF_PATH.format, range(listeners))
            listener_cmds = [f"{dissent_bin} {c}" for c in client_confs]
            proc_futures.extend(
                [asyncio.create_task(_run(client, l, shutdown)) for l in listener_cmds]
            )
        return asyncio.gather(*proc_futures)

    def _parse_log(self, log: str):
        def _time_for_line(line: str):
            time_str = line.partition(" ")[0]
            return datetime.fromisoformat(time_str)

        lines = log.split("\n")
        while lines:
            if not lines:
                raise RuntimeError("No broadcast found in log.")
            line = lines.pop(0)
            # If nobody broadcasts, the round failed!
            if "Opening slot" in line:
                break
        while lines:
            if not lines:
                raise RuntimeError("No start time found in log.")
            line = lines.pop(0)
            # We skip the initial shuffle, since that corresponds to our "setup".
            if "Phase: 1" in line:
                start_time = _time_for_line(line)
                break
        if self.blame:
            for line in lines:
                if "finished bulk" in line:
                    end_time = _time_for_line(line)
                    break
        else:
            for line in lines:
                # If self.blame == False, we skip the blame phase.
                # This corresponds to a best-case run.
                if 'starting: "SERVER_PUSH_CLEARTEXT"' in line:
                    end_time = _time_for_line(line)
                    break
        return Milliseconds(end_time - start_time).total_seconds() * 1000

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        server0 = setting.server0
        server1 = setting.server1
        clients = setting.clients

        await self._cleanup(setting)
        await self._install_keys(list(setting))
        keys = await self._sort_keys(server0)
        ips = await self._sort_ips(server0, server1, clients)

        # set up configs
        counts = await self._install_configs(setting, ips, keys)

        # run the dissent processes
        shutdown = asyncio.Event()
        spinner.text = f"[experiment] starting processes"
        all_procs = self._run_dissent(setting, counts, shutdown)
        try:
            # TODO: wait until broadcaster log has "WaitingForServer"? or "Registering"
            await asyncio.sleep(5)
            curl_cmd = (
                "curl -X POST --data-binary @message localhost:{port}/session/send"
            )
            curl_procs = []
            for (client, (broadcasters, _)) in zip(clients, counts):
                for idx in range(broadcasters):
                    curl_procs.append(
                        client.ssh.run(curl_cmd.format(port=8850 + idx, check=True))
                    )
            await asyncio.gather(*curl_procs)
            wait_time = WAIT_TIME_LONG if self.blame else WAIT_TIME_SHORT
            spinner.text = f"[experiment] run processes for {wait_time}s"
            await server0.ssh.run(
                'tail -f -n +0 server.log | grep -m1 "finished bulk"',
                check=True,
                timeout=wait_time,
            )
            # TODO: wait time should probably be estimated from parameters
            # e.g. it's way too short for many clients + blame
        finally:
            shutdown.set()

        log = (
            await server0.ssh.run(
                "cat server.log | "
                "grep -E '(Opening slot|finished bulk|SERVER_PUSH_CLEARTEXT|Phase: 1)'",
                check=True,
            )
        ).stdout
        latency = self._parse_log(log)
        return Result(experiment=self, time=latency, queries=self.clients)


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
