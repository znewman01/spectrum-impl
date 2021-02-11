import argparse
import itertools
import json
import os
import sys

TRIALS = 1

MESSAGE_SIZES_ONE_CHANNEL = [
    1_000_000,
    2_000_000,
    3_000_000,
    4_000_000,
    5_000_000,
    6_000_000,
    7_000_000,
    8_000_000,
    9_000_000,
    10_000_000,
]
MESSAGE_SIZES_MANY_CHANNEL = [1_000, 5_000, 10_000, 20_000]
CHANNELS = [100, 500, 1000, 2000, 3000, 5000, 8000]

_MULTI_SERVER_CHANNELS = [100]
_MULTI_SERVER_MESSAGE_SIZES = [100_000]
# Doesn't include "full broadcast" plots which are a special case.
PLOTS_SPECTRUM = {
    # "Our turf": one channel, many clients, measure QPS
    "onechannel": {
        "channels": [1],
        "clients": [250],
        "message_size": MESSAGE_SIZES_ONE_CHANNEL,
        "clients_per_machine": [50],
        "workers_per_machine": [2],
    },
    "manychannel": {
        "channels": CHANNELS,
        "clients": [100],
        "message_size": MESSAGE_SIZES_MANY_CHANNEL,
        "workers_per_machine": [4],
    },
    # Horizontal scaling experiment
    "horizontal": {
        "clients": [500],
        "worker_machines_per_group": [1, 2, 3, 4, 5, 6, 7, 8],
        # too many total workers appears to lead to higher tail latencies
        "workers_per_machine": [4],
        "message_size": [100_000],
        "channels": [500],
    },
    # Test the scaling of seed homomorphic protocol
    "multi-server": {
        "clients": [200],
        "clients_per_machine": [50],
        "workers_per_machine": [4],
        "channels": _MULTI_SERVER_CHANNELS,
        "message_size": _MULTI_SERVER_MESSAGE_SIZES,
        "protocol": [
            {"SeedHomomorphic": {"parties": 2,},},
            {"SeedHomomorphic": {"parties": 3,},},
            {"SeedHomomorphic": {"parties": 5,},},
        ],
    },
    "multi-server-control": {
        "clients": [200],
        "workers_per_machine": [4],
        "channels": _MULTI_SERVER_CHANNELS,
        "message_size": _MULTI_SERVER_MESSAGE_SIZES,
    },
}

PLOTS_EXPRESS = {
    "onechannel": {"channels": [1], "message_size": MESSAGE_SIZES_ONE_CHANNEL,},
    "manychannel": {"channels": CHANNELS, "message_size": MESSAGE_SIZES_MANY_CHANNEL,},
}

PLOTS_RIPOSTE = {
    "manychannel": {"channels": CHANNELS, "message_size": MESSAGE_SIZES_MANY_CHANNEL,},
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
        yield experiment


def _write_file(path, data):
    with open(path, "w") as f:
        json.dump(data, f, indent=2)


def main(args):
    parser = argparse.ArgumentParser(args[0])
    parser.add_argument("--trials", type=int, default=1)
    parser.add_argument("output_dir", nargs="?", default=".")
    args = parser.parse_args(args[1:])

    for name, params in PLOTS_SPECTRUM.items():
        path = os.path.join(args.output_dir, f"spectrum-{name}.json")
        experiments = list(make_experiments_spectrum(args.trials, params))
        _write_file(path, experiments)

    for name, params in PLOTS_EXPRESS.items():
        path = os.path.join(args.output_dir, f"express-{name}.json")
        experiments = list(make_experiments(args.trials, params))
        _write_file(path, experiments)

    for name, params in PLOTS_RIPOSTE.items():
        path = os.path.join(args.output_dir, f"riposte-{name}.json")
        experiments = list(make_experiments(args.trials, params))
        _write_file(path, experiments)


if __name__ == "__main__":
    main(sys.argv)
