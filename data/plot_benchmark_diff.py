import matplotlib.pyplot as plt
import pandas as pd
import argparse
import json
import sys

from pathlib import Path
from typing import List


def load_df(path: Path) -> pd.DataFrame:
    with open(path) as f:
        data = json.load(f)
        for result in data:
            result.update(result.pop("experiment"))
        return pd.DataFrame.from_dict(data)


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


def make_means(df: pd.DataFrame, groups: List[str]) -> pd.DataFrame:
    return df.groupby(groups).mean().reset_index()


def plot_manychannel(results_dir: Path, commit_old: str, commit_new: str, show: bool):
    # spectrum = process_spectrum(load_df(results_dir / "spectrum-manychannel.json"))
    # spectrum = spectrum.drop(["worker_machines_per_group", "protocol"], 1)
    # spectrum = make_means(spectrum, ["message_size", "channels"])

    riposte = process_riposte(load_df(results_dir / "riposte-manychannel.json"))
    express = make_means(express, ["message_size", "channels"])

    # big = pd.merge(
    #     express,
    #     spectrum,
    #     how="outer",
    #     on=["message_size", "channels"],
    #     suffixes=["_express", "_spectrum"],
    # )
    big = pd.merge(
        express,  # TODO: -> big
        riposte,
        how="outer",
        on=["message_size", "channels"],
        suffixes=["", "_riposte"],
    )

    sizes = big["message_size"].unique()
    fig, axes = plt.subplots(len(sizes), 1)

    for message_size, ax in zip(sizes, axes):
        little = big[big["message_size"] == message_size].drop("message_size", 1)
        little.plot(
            x="channels",
            title=f"many channel: msg size {message_size}",
            ax=ax,
            marker=".",
            xlim=(big["channels"].min(), big["channels"].max()),
        )

    if show:
        plt.show()


def plot_onechannel(results_dir: Path, commit_old: str, commit_new: str, show: bool):
    data_old = process_spectrum(
        load_df(results_dir / commit_old / "spectrum-onechannel.json")
    )
    data_old = data_old.drop(["channels", "worker_machines_per_group", "protocol"], 1)
    data_old = make_means(data_old, ["message_size"])

    data_new = process_spectrum(
        load_df(results_dir / commit_new / "spectrum-onechannel.json")
    )
    data_new = data_old.drop(["channels", "worker_machines_per_group", "protocol"], 1)
    data_new = make_means(data_old, ["message_size"])

    big = pd.merge(
        data_old, data_new, how="outer", on=["message_size"], suffixes=["_old", "_new"],
    )
    big.plot(x="message_size", title="one channel", marker=".")

    if show:
        plt.show()


# def plot_horizontal(results_dir: Path, show: bool):
#     spectrum = process_spectrum(load_df(results_dir / "spectrum-horizontal.json"))
#     spectrum = spectrum.drop(["message_size", "channels"], 1)
#
#     spectrum.plot(
#         x="worker_machines_per_group", y="qps", title="Horizontal Scaling", marker="."
#     )
#
#     if show:
#         plt.show()


# def plot_multiserver(results_dir: Path, show: bool):
#     seed_homomorphic = process_spectrum(
#         load_df(results_dir / "spectrum-multi-server.json")
#     )
#     seed_homomorphic["servers"] = seed_homomorphic["protocol"].apply(
#         lambda p: p["parties"]
#     )
#     seed_homomorphic = seed_homomorphic.drop(
#         ["protocol", "worker_machines_per_group", "message_size", "channels"], 1
#     )

#     symmetric = process_spectrum(
#         load_df(results_dir / "spectrum-multi-server-control.json")
#     )
#     symmetric = symmetric.drop(
#         ["protocol", "worker_machines_per_group", "message_size", "channels"], 1
#     )
#     symmetric["servers"] = 2

#     big = pd.merge(
#         symmetric,
#         seed_homomorphic,
#         how="outer",
#         on=["servers"],
#         suffixes=["_symmetric", "_seed_homomorphic"],
#     )

#     big.plot.bar(x="servers")

#     if show:
#         plt.show()


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument(
        "--results-dir", required=True, help="Base directory for results to compare"
    )
    parser.add_argument("--baseline", required=True, help="Baseline commit")
    parser.add_argument("--new", required=True, help="New commit")
    # parser.add_argument('--output', help="New commit")
    args = parser.parse_args(args[1:])

    # if args.output:
    #     fig.savefig(args.output)
    # else:
    #     plt.show(fig)


if __name__ == "__main__":
    main(sys.argv)
