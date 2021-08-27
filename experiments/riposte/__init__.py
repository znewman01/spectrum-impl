from __future__ import annotations

import asyncio
import math
import re

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


RESULT_RE = r"Served (?P<queries>\d*) requests at (?P<rate>[\d.]*) reqs/sec"
WAIT_TIME = 60
HOME = Path("/home/ubuntu")
RIPOSTE_BASE = HOME / "go/src/bitbucket.org/henrycg/riposte"
PORT = 4000


@dataclass
class Setting(system.Setting):
    clients: List[Machine]
    leader: Machine
    server: Machine
    auditor: Machine

    @staticmethod
    def to_machine_spec(
        tf_data: Dict[str, Any]
    ) -> Dict[Union[str, Tuple[str, int]], str]:
        result = {}
        for name in ("leader", "server", "auditor"):
            result[name] = tf_data[name]
        for idx, client in enumerate(tf_data["clients"]):
            result[("client", idx)] = client
        return result

    @classmethod
    def from_dict(cls, machines: Dict[Any, Machine]) -> Setting:
        leader = None
        server = None
        auditor = None
        clients = []
        for ident, machine in machines.items():
            if ident == "leader":
                leader = machine
            elif ident == "server":
                server = machine
            elif ident == "auditor":
                auditor = machine
            elif ident[0] == "client":
                clients.append(machine)
            else:
                raise ValueError(f"Invalid identifier [{ident}]")
        if leader is None or server is None or auditor is None or not clients:
            raise ValueError(f"Missing machines; got {machines}")
        return cls(clients, leader, server, auditor)

    async def additional_setup(self):
        pass

    def __iter__(self):
        return iter([self.leader, self.server, self.auditor] + self.clients)


@dataclass(order=True, frozen=True)
class Environment(system.Environment):
    instance_type: InstanceType

    def make_tf_vars(self, _build: Optional[packer.Build], _: Any) -> Dict[str, Any]:
        tf_vars = {
            "region": AWS_REGION,
            "instance_type": self.instance_type,
        }
        return tf_vars

    @staticmethod
    def make_tf_cleanup_vars():
        return {
            "region": AWS_REGION,  # must be the same
            "instance_type": DEFAULT_INSTANCE_TYPE,
        }


@dataclass(frozen=True)
class Experiment(system.Experiment):
    instance_type: InstanceType = DEFAULT_INSTANCE_TYPE
    server_threads: int = 8  # num cores
    client_threads: int = 16  # 2 * (num cores)
    channels: int = 1
    message_size: Bytes = Bytes(160)
    # TODO: >2 servers?

    def to_environment(self) -> Environment:
        return Environment(instance_type=self.instance_type)

    @classmethod
    def from_dict(cls, data) -> Experiment:
        if "message_size" in data:
            data["message_size"] = Bytes(data["message_size"])
        return cls(**data)

    async def _compile_machine(self, machine: Machine, width: int, height: int):
        # Patch our template types.go file in the Riposte source tree
        env_vars = {
            "TABLE_WIDTH": str(width),
            "TABLE_HEIGHT": str(height),
            "MESSAGE_SIZE": str(self.message_size),
        }
        env_spec = " ".join(
            [f"${key}" for key in env_vars]
        )  # envsubst wants '$FOO $BAR'
        # ssh.run(env=) doesn't work here, so specify environment variables inline
        env = " ".join([f"{key}={value}" for key, value in env_vars.items()])
        template_path = HOME / "config" / "types.go.template"
        source_path = RIPOSTE_BASE / "db" / "types.go"
        await machine.ssh.run(
            f"{env} envsubst '{env_spec}' < {template_path} > {source_path}"
        )

        # Compile all the binaries (we have to go into each directory)
        for binary_dir in ("server", "client"):
            path = RIPOSTE_BASE / binary_dir
            await machine.ssh.run(f"cd {path} && go build", check=True)

    async def _compile(self, setting: Setting, width: int, height: int):
        tasks = [self._compile_machine(m, width, height) for m in setting]
        await asyncio.gather(*tasks)

    def _parse(self, server_output: List[str]) -> Result:
        log_path = Path("riposte.log")
        with open(log_path, "w") as log_file:
            for line in server_output:
                log_file.write(line + "\n")

        matches = list(filter(None, map(partial(re.search, RESULT_RE), server_output)))
        total_time = 0.0
        total_queries = 0
        count = len(matches)
        if count <= 2:
            raise ValueError(
                f"Output from server contains only {count} indications of performance "
                f"(output in {log_path})"
            )
        # We modified Riposte to report marginal, rather than cumulative,
        # queries/time. So we can sum accross.
        for match in matches[1:]:
            queries = int(match.group("queries"))
            rate = float(match.group("rate"))
            try:
                time = queries / rate
            except ZeroDivisionError:
                continue
            total_queries += int(match.group("queries"))
            total_time += time

        if total_queries == 0:
            raise RuntimeError("Riposte did not succeed")

        return Result(
            experiment=self,
            time=Milliseconds(int(total_time * 1000)),
            queries=total_queries,
        )

    async def _run(self, setting: Setting, spinner: Halo) -> Result:
        leader = setting.leader
        server = setting.server
        auditor = setting.auditor

        spinner.text = "[experiment] starting servers"
        hosts = ",".join([f"{m.hostname}:{PORT}" for m in (leader, server, auditor)])
        server_cmd = (
            f"ulimit -n 65536 && "
            f"{RIPOSTE_BASE}/server/server -idx {{idx}} "
            f"    -servers {hosts} "
            f"    -threads {self.server_threads} "
            f"2>&1 "
            f"| tee /tmp/riposte.log"
        )
        auditor_proc = auditor.ssh.create_process(server_cmd.format(idx=2))
        server_proc = server.ssh.create_process(server_cmd.format(idx=1))
        leader_proc = leader.ssh.create_process(server_cmd.format(idx=0))
        # order of below is important
        async with auditor_proc as auditor_proc:
            async with server_proc as server_proc:
                await asyncio.sleep(1)  # leader needs other servers to be up
                async with leader_proc as leader_proc:
                    await asyncio.sleep(2)  # leader waits 2s at the beginning

                    spinner.text = "[experiment] starting clients"
                    client_cmd = (
                        f"{RIPOSTE_BASE}/client/client "
                        f"    -leader {leader.hostname}:{PORT} "
                        f"    -hammer "
                        f"    -threads {self.client_threads} "
                        f"2>&1 "
                        f"| tee /tmp/riposte-client.log"
                    )
                    client_procs = await asyncio.gather(
                        *[c.ssh.create_process(client_cmd) for c in setting.clients]
                    )

                    spinner.text = f"[experiment] run experiment for {WAIT_TIME}s"
                    await asyncio.sleep(WAIT_TIME)

                    spinner.text = "[experiment] cleaning up"
                    for client_proc in client_procs:
                        client_proc.kill()
                    for client_proc in client_procs:
                        await client_proc.wait()
                    leader_proc.kill()
                    server_proc.kill()
                    auditor_proc.kill()

        spinner.text = "[experiment] parsing output"
        lines = (await leader_proc.stdout.read()).split("\n")
        return self._parse(lines)

    async def run(self, setting: Setting, spinner: Halo) -> Result:
        # See Riposte sec. 3.2 for how to calculate number of writers that we
        # can handle. This gives a 95% success rate.
        # The Riposte implementation uses XOR, not field addition so we need the
        # 19.5 multiplier not 2.7.
        rows = math.ceil(self.channels * 19.5)

        # See Riposte sec. 4.3 for how to calculate communication-optimal width/height
        # these variable names correspond to that section
        alpha = 128
        beta = self.message_size * 8  # bits per byte
        c = math.sqrt(beta / (1 + alpha))  # pylint: disable=invalid-name
        height_optimal = math.ceil(math.sqrt(rows) * c)
        width_optimal = math.ceil(math.sqrt(rows) / c)

        # But Riposte fig. 4 suggests width = height is optimal
        width_even = height_even = math.ceil(math.sqrt(rows))

        results = []
        for width, height in (
            (width_optimal, height_optimal),
            (width_even, height_even),
        ):
            # Riposte has no configuration files, so we need to recompile
            spinner.text = "[experiment] compiling with correct settings"
            await self._compile(
                setting, width, height
            )  # TODO(zjn): catch if this doesn't work
            results.append(await self._run(setting, spinner))

        # return the best result
        return max(*results, key=lambda r: r.qps)


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


RIPOSTE = system.System(
    environment=Environment,
    experiment=Experiment,
    setting=Setting,
    packer_config=PackerConfig,
    root_dir=Path(__file__).parent,
)
