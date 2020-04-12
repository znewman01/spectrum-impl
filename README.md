# spectrum-impl

[![Build Status](https://travis-ci.com/znewman01/spectrum-impl.svg?token=osr5byrKJvECZutBPrRq&branch=master)](https://travis-ci.com/znewman01/spectrum-impl)

Implementation for [Spectrum paper](https://github.com/sachaservan/spectrum-paper).

See [design document](https://docs.google.com/document/d/1Z8g1ovBGFthpsDLR_88Pn4-9tKX_QnbV0ZSba2UwXno/edit#).

## Usage

For local development, the primary entry point is `cargo run --bin run_inmem`, which
will run all of the parties in the protocol locally. Use `--help` to see
parameters.

We can run some quick local benchmarks of the whole system with `cargo run --bin run_processes`.
Make sure to use `--release`, and maybe set `ulimit -n 8192`.
By default, this runs some hard-coded parameters, but we accept arbitrary inputs in CSV format:

```
$ cat input.csv
2,1,50,8,,1250000
2,1,50,8,63,1250000
$ cat input.csv |
    cargo run \
        --release \
        --bin run_processes \
        -- \
        --output data.csv \
        --input -
Running: InputRecord { groups: 2, group_size: 1, clients: 50, channels: 8, security_bits: None, msg_size: 1250000 }...done. elapsed time 293.903964ms
Running: InputRecord { groups: 2, group_size: 1, clients: 50, channels: 8, security_bits: Some(63), msg_size: 1250000 }...done. elapsed time 600.449376ms
$ cat data.csv
groups,group_size,clients,channels,security_bits,msg_size,elapsed_millis
2,1,50,8,,1250000,293
2,1,50,8,60,1250000,600
```

This mode works interactively, too! Though for one-off executions `run_inmem` might
be better.
