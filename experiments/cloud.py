# encoding: utf8
import json
import operator
import subprocess

from contextlib import contextmanager
from functools import reduce
from pathlib import Path
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory
from typing import NewType, Dict, Any, List

from halo import Halo

from experiments.system import System


Region = NewType("Region", str)
SHA = NewType("SHA", str)
AMI = NewType("AMI", str)
InstanceType = NewType("InstanceType", str)

AWS_REGION = Region("us-east-2")


def format_args(var_dict: Dict[str, Any]) -> List:
    return reduce(operator.add, [["-var", f"{k}={v}"] for k, v in var_dict.items()])


@contextmanager
def terraform(tf_vars: Dict[str, Any], tf_dir: Path):
    with TemporaryDirectory() as tmpdir:
        with Halo("[infrastructure] checking current state") as spinner:
            plan = Path(tmpdir) / "tfplan"
            tf_args = format_args(tf_vars)
            cmd = ["terraform", "plan", f"-out={plan}", "-no-color"] + tf_args
            try:
                plan_output = check_output(cmd, stderr=subprocess.STDOUT, cwd=tf_dir)
            except subprocess.CalledProcessError as err:
                if "terraform init" in err.output.decode("utf8"):
                    # we know what to do here
                    spinner.text = "[infrastructure] initializing plugins"
                    check_output(["terraform", "init"], cwd=tf_dir)
                    spinner.text = "[infrastructure] checking current state"
                    plan_output = check_output(cmd, cwd=tf_dir)
                else:
                    raise
            changes = [
                l
                for l in plan_output.decode("utf8").split("\n")
                if l.lstrip().startswith("#")
            ]

            if changes:
                spinner.succeed("[infrastructure] found changes to apply:")
                for change in changes:
                    change = change.lstrip(" #")
                    print(f"  • {change}")
            else:
                spinner.info("[infrastructure] no changes to apply")

        if changes:
            with Halo(
                "[infrastructure] applying changes (output in [terraform.log])"
            ) as spinner:
                with open("terraform.log", "w") as log_file:
                    cmd = [
                        "terraform",
                        "apply",
                        "-refresh=false",
                        "-auto-approve",
                        str(plan),
                    ]
                    check_call(cmd, stdout=log_file, cwd=tf_dir)
                spinner.succeed("[infrastructure] created")

        data = json.loads(check_output(["terraform", "output", "-json"], cwd=tf_dir))
    yield {k: v["value"] for k, v in data.items()}


@contextmanager
def cleanup(system: System):
    try:
        yield
    finally:
        tf_vars = system.environment.make_tf_cleanup_vars()
        tf_args = format_args(tf_vars)
        with Halo("[infrastructure] tearing down all resources") as spinner:
            check_call(
                ["terraform", "destroy", "-auto-approve"] + tf_args,
                stdout=subprocess.DEVNULL,
                cwd=system.root_dir,
            )
            spinner.succeed()
