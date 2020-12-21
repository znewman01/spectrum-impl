import argparse
import itertools
import json
import os
import sys

TRIALS = 5

MESSAGE_SIZES = [1_000, 10_000, 50_000, 100_000, 500_000, 1_000_000]
CHANNELS = [100, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000]

# Doesn't include "full broadcast" plots which are a special case.
PLOTS_SPECTRUM = {
    # "Our turf": one channel, many clients, measure QPS
    "onechannel": {"channels": [1], "clients": [1000], "message_size": MESSAGE_SIZES,},
    # "Their turf": number clients = number channels
    "manychannel": {"channels": CHANNELS, "message_size": MESSAGE_SIZES,},
    # Horizontal scaling experiment
    "horizontal": {
        "worker_machines_per_group": [1, 2, 3, 4, 5, 6, 7, 8],
        # too many total workers appears to lead to higher tail latencies
        "workers_per_machine": [4],
        "message_size": [100_000],
        "clients": [1500],
        "channels": [500],
    },
    # Test the scaling of seed homomorphic protocol
    "multi-server": {
        "clients": [1400],
        "clients_per_machine": [200],
        "channels": [100],
        "message_size": [100_000],
        "protocol": [
            {"SeedHomomorphic": {"parties": 2,},},
            {"SeedHomomorphic": {"parties": 3,},},
            {"SeedHomomorphic": {"parties": 5,},},
        ],
    },
    "multi-server-control": {
        "clients": [1400],
        "channels": [100],
        "message_size": [100_000],
        "protocol": [{"Symmetric": {"security": 16,}}],
    },
}

PLOTS_EXPRESS = {
    "onechannel": {"channels": [1], "message_size": MESSAGE_SIZES,},
    "manychannel": {"channels": CHANNELS, "message_size": MESSAGE_SIZES,},
}

PLOTS_RIPOSTE = {
    "manychannel": {"channels": CHANNELS, "message_size": MESSAGE_SIZES,},
}


def make_experiments(trials, params):
    # params is dict of key:list
    keys = list(params)
    for values in itertools.product(*[params[k] for k in keys]):
        for _ in range(trials):
            yield dict(zip(keys, values))


def make_experiments_spectrum(trials, params):
    # support for "full broadcast" plots (# channels = # clients) by omitting clients/channels
    # other services don't need this
    for experiment in make_experiments(trials, params):
        if "clients" in experiment and "channels" not in experiment:
            experiment["channels"] = experiment["clients"]
        elif "channels" in experiment and "clients" not in experiment:
            experiment["client"] = experiment["channels"]
        yield experiment


def _write_file(path, data):
    with open(path, "w") as f:
        json.dump(data, f, indent=2)


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument("output_dir", nargs="?", default=".")
    args = parser.parse_args(args[1:])

    for name, params in PLOTS_SPECTRUM.items():
        path = os.path.join(args.output_dir, f"spectrum-{name}.json")
        experiments = list(make_experiments_spectrum(TRIALS, params))
        _write_file(path, experiments)

    for name, params in PLOTS_EXPRESS.items():
        path = os.path.join(args.output_dir, f"express-{name}.json")
        experiments = list(make_experiments(TRIALS, params))
        _write_file(path, experiments)

    for name, params in PLOTS_EXPRESS.items():
        path = os.path.join(args.output_dir, f"riposte-{name}.json")
        experiments = list(make_experiments(TRIALS, params))
        _write_file(path, experiments)


if __name__ == "__main__":
    main(sys.argv)
