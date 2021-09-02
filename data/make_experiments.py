import argparse
import itertools
import json
import os
import sys

FEW_CHANNELS = [1]
MESSAGE_SIZES_FEW_CHANNELS = [
    100_000,
    500_000,
    1_000_000,
    1_500_000,
    2_000_000,
    2_500_000,
    3_000_000,
    3_500_000,
    4_000_000,
    4_500_000,
    5_000_000,
]

MESSAGE_SIZES_MANY_CHANNELS = [1_000, 5_000, 10_000]
MANY_CHANNELS = [
    1000,
    2000,
    3000,
    5000,
    8000,
    10000,
]

MULTI_SERVER_CHANNELS = [64]
MULTI_SERVER_MESSAGE_SIZES = [160]

# Doesn't include "full broadcast" plots which are a special case.
PLOTS_SPECTRUM = {
    "onechannel": {
        "clients": [64],
        "channels": FEW_CHANNELS,
        "message_size": MESSAGE_SIZES_FEW_CHANNELS,
    },
    "manychannel": {
        "clients": [64],
        "channels": MANY_CHANNELS,
        "message_size": MESSAGE_SIZES_MANY_CHANNELS,
    },
    # Horizontal scaling experiment
    "horizontal": {
        "clients": [64],
        "worker_machines_per_group": [1, 2, 3, 5, 8, 10],
        "message_size": [100_000],
        "channels": [500],
    },
    # Test the scaling of seed homomorphic protocol
    "multi-server": {
        "clients": [64],
        "clients_per_machine": [32],
        "channels": MULTI_SERVER_CHANNELS,
        "message_size": MULTI_SERVER_MESSAGE_SIZES,
        "protocol": [
            {"SeedHomomorphic": {"parties": 2}},
            {"SeedHomomorphic": {"parties": 4}},
            {"SeedHomomorphic": {"parties": 6}},
            {"SeedHomomorphic": {"parties": 8}},
            {"SeedHomomorphic": {"parties": 10}},
        ],
    },
    "multi-server-control": {
        "clients": [64],
        "channels": MULTI_SERVER_CHANNELS,
        "message_size": MULTI_SERVER_MESSAGE_SIZES,
    },
}

PLOTS_EXPRESS = {
    "onechannel": {
        "channels": FEW_CHANNELS,
        "message_size": MESSAGE_SIZES_FEW_CHANNELS,
    },
    "manychannel": {
        "channels": MANY_CHANNELS,
        "message_size": MESSAGE_SIZES_MANY_CHANNELS,
    },
}

PLOTS_RIPOSTE = {
    # riposte can't separate channels and users, so we just take the max
    # also it barfs with messages >100KB (won't compile) so just do the manychannel setting
    "manychannel": {
        "channels": [max(MANY_CHANNELS)],
        # I've tried many times, it doesn't report any progress for >=5KB messages.
        "message_size": [s for s in MESSAGE_SIZES_MANY_CHANNELS if s < 5000],
    },
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
