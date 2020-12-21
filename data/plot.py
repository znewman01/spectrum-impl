import matplotlib.pyplot as plt
import pandas as pd
import argparse
import json
import sys

from typing import List, Optional, Tuple
from pathlib import Path


def load_df(path: Path) -> pd.DataFrame:
    with open(path) as f:
        data = json.load(f)
        for result in data:
            result.update(result.pop("experiment"))
        return pd.DataFrame.from_dict(data)


def merge_many(dfs: List[Tuple[str, pd.DataFrame]], on=List[str]) -> pd.DataFrame:
    res = pd.DataFrame()
    for column in on + ["qps"]:
        res[column] = []
    for name, df in dfs:
        res = pd.merge(res, df, how="outer", on=on, suffixes=["", f"_{name}"])
    res = res.drop("qps", 1)
    if "qps_" in res.columns:
        res = res.rename(columns={"qps_": "qps"})
    return res


def process_express(df: pd.DataFrame) -> pd.DataFrame:
    df = df.drop(
        ["client_threads", "server_threads", "instance_type", "time", "queries"], 1
    )
    return df


def process_spectrum(df: pd.DataFrame) -> pd.DataFrame:
    df = df.drop(
        [
            "clients_per_machine",
            "workers_per_machine",
            # "protocol",
            "instance_type",
            "clients",
            "time",
            "queries",
        ],
        1,
    )
    return df


def process_riposte(df: pd.DataFrame) -> pd.DataFrame:
    df = df.drop(
        ["client_threads", "server_threads", "instance_type", "time", "queries"], 1
    )
    return df


def make_means(df: pd.DataFrame, groups: List[str]) -> pd.DataFrame:
    return df.groupby(groups).mean().reset_index()


def plot_manychannel(results_dir: Path, benchmark: Optional[List[str]], show: bool):
    dfs = []
    if benchmark:
        old = load_df(results_dir / benchmark[0] / "spectrum-manychannel.json")
        old = process_spectrum(old).drop(["worker_machines_per_group", "protocol"], 1)
        old = make_means(old, ["message_size", "channels"])
        dfs.append(("old", old))

        new = load_df(results_dir / benchmark[0] / "spectrum-manychannel.json")
        new = process_spectrum(new).drop(["worker_machines_per_group", "protocol"], 1)
        new = make_means(new, ["message_size", "channels"])
        dfs.append(("new", new))
    else:
        express = process_express(load_df(results_dir / "express-manychannel.json"))
        express = make_means(express, ["message_size", "channels"])
        dfs.append(("express", express))

        spectrum = process_spectrum(load_df(results_dir / "spectrum-manychannel.json"))
        spectrum = spectrum.drop(["worker_machines_per_group", "protocol"], 1)
        spectrum = make_means(spectrum, ["message_size", "channels"])
        dfs.append(("spectrum", spectrum))

        riposte = process_riposte(load_df(results_dir / "riposte-manychannel.json"))
        riposte = make_means(riposte, ["message_size", "channels"])
        dfs.append(("riposte", riposte))

    big = merge_many(dfs, on=["message_size", "channels"])
    print(big)

    sizes = big["message_size"].unique()
    _, axes = plt.subplots(len(sizes), 1)
    for message_size, ax in zip(sizes, axes):
        little = big[big["message_size"] == message_size].drop("message_size", 1)
        little.plot(
            x="channels",
            title=f"many channel: message_size size {message_size}",
            ax=ax,
            marker=".",
            xlim=(big["channels"].min(), big["channels"].max()),
        )

    if show:
        plt.show()


def plot_onechannel(results_dir: Path, benchmark: Optional[List[str]], show: bool):
    if benchmark:
        old = process_spectrum(
            load_df(results_dir / benchmark[0] / "spectrum-onechannel.json")
        )
        old = old.drop(["channels", "worker_machines_per_group", "protocol"], 1)
        old = make_means(old, ["message_size"])

        new = process_spectrum(
            load_df(results_dir / benchmark[1] / "spectrum-onechannel.json")
        )
        new = new.drop(["channels", "worker_machines_per_group", "protocol"], 1)
        new = make_means(new, ["message_size"])

        dfs = [("old", old), ("new", new)]
    else:
        express = process_express(load_df(results_dir / "express-onechannel.json"))
        express = express.drop("channels", 1)
        express = make_means(express, ["message_size"])

        spectrum = process_spectrum(load_df(results_dir / "spectrum-onechannel.json"))
        spectrum = spectrum.drop(
            ["channels", "worker_machines_per_group", "protocol"], 1
        )
        spectrum = make_means(spectrum, ["message_size"])

        dfs = [("express", express), ("spectrum", spectrum)]

    big = merge_many(dfs, on=["message_size"])
    big.plot(x="message_size", title="one channel", marker=".")

    if show:
        plt.show()


def plot_horizontal(results_dir: Path, benchmark: Optional[List[str]], show: bool):
    if benchmark:
        old = load_df(results_dir / benchmark[0] / "spectrum-horizontal.json")
        old = process_spectrum(old).drop(["message_size", "channels"], 1)

        new = load_df(results_dir / benchmark[1] / "spectrum-horizontal.json")
        new = process_spectrum(new).drop(["message_size", "channels"], 1)

        dfs = [("old", old), ("new", new)]
    else:
        spectrum = process_spectrum(load_df(results_dir / "spectrum-horizontal.json"))
        spectrum = spectrum.drop(["message_size", "channels"], 1)
        dfs = [("", spectrum)]

    big = merge_many(dfs, on=["worker_machines_per_group"])

    big.plot(x="worker_machines_per_group", title="Horizontal Scaling", marker=".")

    if show:
        plt.show()


def plot_multiserver(results_dir: Path, benchmark: Optional[List[str]], show: bool):
    dfs = []
    for commit in benchmark or [""]:
        if commit:
            results_subdir = results_dir / commit
        else:
            results_subdir = results_dir
        seed_homomorphic = process_spectrum(
            load_df(results_subdir / "spectrum-multi-server.json")
        )
        seed_homomorphic["servers"] = seed_homomorphic["protocol"].apply(
            lambda p: p["parties"]
        )
        seed_homomorphic = seed_homomorphic.drop(
            ["protocol", "worker_machines_per_group", "message_size", "channels"], 1
        )

        symmetric = process_spectrum(
            load_df(results_subdir / "spectrum-multi-server-control.json")
        )
        symmetric = symmetric.drop(
            ["protocol", "worker_machines_per_group", "message_size", "channels"], 1
        )
        symmetric["servers"] = 2

        if benchmark and commit == benchmark[0]:
            suffix = "_old"
        elif benchmark and commit == benchmark[1]:
            suffix = "_new"
        elif commit == "":
            suffix = ""
        else:
            suffix = f"_{commit}"
        dfs.append((f"symmetric{suffix}", symmetric))
        dfs.append((f"seed_homomorphic{suffix}", seed_homomorphic))

    big = merge_many(dfs, on=["servers"])
    big.plot.bar(x="servers")

    if show:
        plt.show()


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument(
        "--results-dir", required=True, help="Base directory for results to compare"
    )
    parser.add_argument("--benchmark", help="commits: [OLD:NEW]")
    parser.add_argument("--show", action="store_true", help="Show plots")
    args = parser.parse_args(args[1:])

    results_dir = Path(args.results_dir)
    benchmark = args.benchmark.split(":") if args.benchmark else None

    plot_onechannel(results_dir, benchmark, show=args.show)
    plot_manychannel(results_dir, benchmark, show=args.show)
    plot_horizontal(results_dir, benchmark, show=args.show)
    plot_multiserver(results_dir, benchmark, show=args.show)
    # TODO: bandwidth
    # TODO: cost
    # TODO: microbenchmark


if __name__ == "__main__":
    main(sys.argv)
