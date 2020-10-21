import argparse
import itertools
import json
import os
import sys

TRIALS = 5

# Doesn't include "full broadcast" plots which are a special case.
PLOTS = {
    # 10 kbit--1 Mbit messages
    # 1, 5, 10, 25, 50, 100 channels
    "medium": {
        "channels": [1, 50, 100],
        "clients": [4000],
        "message_size": [1_250, 12_500, 25_000, 50_000, 125_000],
    },

    # 1--8 Mbit messages
    # 1, 5, 10 channels
    "large": {
        "channels": [1, 10, 50],
        "clients": [1000],
        "message_size": [125_000, 250_000, 500_000, 1_000_000],
    },

    # Horizontal scaling experiment
    "horizontal": {
        "worker_machines_per_group": [1, 2, 3, 4, 5, 6, 7, 8],
        # too many total workers appears to lead to higher tail latencies
        "workers_per_machine": [4],
        "message_size": [125_000],
        "clients": [1500],
        "channels": [500],
    },

    # Test the scaling of seed homomorphic protocol
    "multi-server": {
        "clients": [1400],
        "clients_per_machine": [200],
        "channels": [100],
        "message_size": [125_000],
        "protocol": [
            {
                "SeedHomomorphic": {
                    "parties": 2,
                },
            },
            {
                "SeedHomomorphic": {
                    "parties": 3,
                },
            },
            {
                "SeedHomomorphic": {
                    "parties": 5,
                },
            },
        ]
    },
    "multi-server-control": {
        "clients": [1400],
        "channels": [100],
        "message_size": [125_000],
        "protocol": [
            {
                "Symmetric": {
                    "security": 16,
                }
            }
        ]
    }
}

# Special case because we force # clients == # channels
FULL_BROADCAST_PLOTS = {
    "small": {
        "clients": [1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000],
        "message_size": [100, 1000, 10000]
    }
}

BENCHMARK_PLOTS = {
    "small": {
        "clients": [8000],
        "channels": [8000],
        "message_size": [100, 1000, 10000]
    },
    "large": {
        "clients": [1000],
        "channels": [1],
        "message_size": [100_000, 1_000_000, 10_000_000]
    },
    "multi-server": PLOTS["multi-server"],
    "multi-server-control": PLOTS["multi-server-control"]
}


def make_experiments(trials, params):
    # params is dict of key:list
    keys = list(params)
    for values in itertools.product(*[params[k] for k in keys]):
        for _ in range(trials):
            yield dict(zip(keys, values))


def make_full_broadcast_experiments(trials, params):
    for experiment in make_experiments(trials, params):
        if "clients" in experiment:
            assert "channels" not in experiment
            experiment["channels"] = experiment["clients"]
        elif "channels" not in experiment:
            assert "channels" not in experiment
            experiment["channels"] = experiment["clients"]
        else:
            assert False
        yield experiment


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument('output_dir', nargs="?", default=".")
    args = parser.parse_args(args[1:])

    for name, params in PLOTS.items():
        path = os.path.join(args.output_dir, f"experiments-{name}.json")
        with open(path, "w") as f:
            experiments = list(make_experiments(TRIALS, params))
            json.dump(experiments, f, indent=2)

    for name, params in BENCHMARK_PLOTS.items():
        path = os.path.join(args.output_dir, f"benchmarks-{name}.json")
        with open(path, "w") as f:
            experiments = list(make_experiments(TRIALS, params))
            json.dump(experiments, f, indent=2)

    for name, params in FULL_BROADCAST_PLOTS.items():
        path = os.path.join(args.output_dir, f"experiments-{name}.json")
        with open(path, "w") as f:
            experiments = list(make_full_broadcast_experiments(TRIALS, params))
            json.dump(experiments, f, indent=2)


if __name__ == "__main__":
    main(sys.argv)
