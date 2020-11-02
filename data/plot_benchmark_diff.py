import matplotlib.pyplot as plt
import pandas as pd
import argparse
import json
import sys
import os
import re
from glob import glob


def results_for_commit(results_dir, sha):
    paths = glob(f"{results_dir}/{sha}-benchmarks-*.json")
    results = {}
    for path in paths:
        fname = os.path.basename(path)
        match = re.match("[0-9a-f]{6}-benchmarks-(.*).json", fname)
        result_type = match.group(1)
        results[result_type] = path
        results[result_type + "2"] = path
    return results


def load_df(path):
    with open(path) as f:
        data = json.load(f)
    for result in data:
        result.update(result.pop("experiment"))
    return pd.DataFrame.from_dict(data)


def plot(baseline, new):
    if set(baseline.keys()) != set(new.keys()):
        # TODO: better error
        raise Exception("bad: differing results")

    fig, axs = plt.subplots(len(baseline), 1)
    for result_type, ax in zip(baseline, axs):
        df1 = load_df(baseline[result_type]).drop(["workers_per_machine", "clients_per_machine", "worker_machines_per_group"], axis=1)
        df2 = load_df(new[result_type]).drop(["workers_per_machine", "clients_per_machine", "worker_machines_per_group"], axis=1)
        df1_grouped = df1.groupby(["message_size", "clients", "channels"])
        df2_grouped = df2.groupby(["message_size", "clients", "channels"])
        means = pd.merge(
            df1_grouped.mean().rename(columns={"time": "old"}),
            df2_grouped.mean().rename(columns={"time": "new"}),
            left_index=True, right_index=True
        )
        errs = pd.merge(
            df1_grouped.std().rename(columns={"time": "old"}),
            df2_grouped.std().rename(columns={"time": "new"}),
            left_index=True, right_index=True
        )
        means.plot.bar(yerr=errs, capsize=4, rot=0, ax=ax, title=result_type)
    return fig


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument('--results-dir', required=True, help="Base directory for results to compare")
    parser.add_argument('--baseline', required=True, help="Baseline commit")
    parser.add_argument('--new', required=True, help="New commit")
    parser.add_argument('--output', help="New commit")
    args = parser.parse_args(args[1:])

    baseline_results = results_for_commit(args.results_dir, args.baseline)
    new_results = results_for_commit(args.results_dir, args.new)

    fig = plot(baseline_results, new_results)
    if args.output:
        fig.savefig(args.output)
    else:
        plt.show(fig)

if __name__ == "__main__":
    main(sys.argv)
