///! Module containing experiment infrastructure.
///!
///! To submit
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::time::Duration;

// TODO: this can all go away when serde has support for default literals.
// https://github.com/serde-rs/serde/issues/368

fn ami_0fc20dd1da406780b() -> String {
    "ami-0fc20dd1da406780b".to_string()
}

fn _100() -> u16 {
    100
}

fn _1() -> u16 {
    1
}

fn _16() -> u16 {
    16
}

fn c5_metal() -> MachineType {
    MachineType {
        instance_type: "c5_metal".to_string(),
    }
}
fn m5_large() -> MachineType {
    MachineType {
        instance_type: "m5_large".to_string(),
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
#[serde(transparent)]
struct MachineType {
    pub instance_type: String,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
struct MachineTypeConfiguration {
    /// Machine type of the publisher/etcd machine
    #[serde(default = "m5_large")]
    publisher: MachineType,

    /// Machine type of the server for running workers/leaders.
    #[serde(default = "c5_metal")]
    worker: MachineType,

    /// Machine type of the server for simulating clients.
    #[serde(default = "m5_large")]
    client: MachineType,
}

impl Default for MachineTypeConfiguration {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// Configuration for the environment in which to run an experiment.
///
/// As a rule of thumb, anything that can requires a change in infrastructure
/// (machines set-up/torn-down, files written, services started/stopped) should
/// go here.
#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
pub struct Environment {
    /// The AWS instance types to use for each type of VM in the experiment.
    #[serde(default)]
    machine_types: MachineTypeConfiguration,

    /// Maximum number of clients a given machine should simulate
    #[serde(default = "_100")]
    clients_per_machine: u16,

    /// Amazon AMI ID to use as the base image for experiments.
    ///
    /// The default is a recent (as of 2020-03-30) build of Ubuntu server 18.04.
    #[serde(default = "ami_0fc20dd1da406780b")]
    base_ami: String,

    // TODO: AWS region.
    /// Number of worker machines per group.
    #[serde(default = "_1")]
    group_size: u16,

    // TODO(zjn): move the following to Experiment when we can change them at runtime.
    /// Number of clients to simulate.
    clients: u32,

    /// Number of channels for the protocol.
    channels: u16,

    // The size, in bytes, for each message.
    message_size: u16,

    // The protocol to run.
    //
    // Note that this encapsulates the number of trust groups.
    protocol: Protocol,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
enum Protocol {
    /// The main Spectrum protocol: two parties, using a standard cryptographic PRG.
    Symmetric {
        /// The security level, in bytes, for the protocol.
        ///
        /// Default is 128 bits.
        #[serde(default = "_16")]
        security: u16,
    },

    /// An insecure variation of the Spectrum protocol.
    ///
    /// In this protocol, data is sent in the clear.
    /// Used only for benchmarking (e.g. network throughput).
    Insecure {
        /// Number of "trust groups" to simulate.
        parties: u16,
    },

    /// Generalization of the Spectrum protocol to support many parties.
    ///
    /// Uses a seed-homomorphic PRG and the JubJub group.
    SeedHomomorphic { parties: u16 },
}

/// A configuration for an experiment to run.
#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
pub struct Experiment {
    /// The environment (infrastructure etc.) in which to run an experiment.
    ///
    /// Experiments will be grouped by environment and run on the same infrastructure.
    #[serde(flatten)]
    environment: Environment,
}

impl Experiment {
    /// Returns an iterator over all AWS instance types for all VM types in this experiment.
    pub fn instance_types(&self) -> Vec<String> {
        let machines = &self.environment.machine_types;
        vec![
            machines.publisher.instance_type.clone(),
            machines.worker.instance_type.clone(),
            machines.client.instance_type.clone(),
        ]
    }

    pub fn by_environment(experiments: Vec<Experiment>) -> HashMap<Environment, Vec<Experiment>> {
        experiments
            .into_iter()
            .map(|e| (e.environment.clone(), e))
            .into_group_map()
    }
}

/// Outcome of an experiment.
#[derive(Serialize, Deserialize, Debug)]
pub struct Result {
    /// The experiment that was run.
    experiment: Experiment,

    /// The experiment running time.
    ///
    /// Time is measured from the point at which the clients begin sending data)
    /// to the point at which the publisher has recovered the message.
    ///
    /// This is expected to be sufficient for all plots/tables.
    time: Duration,
}

impl Result {
    pub fn new(experiment: Experiment, time: Duration) -> Self {
        Result { experiment, time }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::prelude::*;
    use std::io::BufReader;
    use std::path::Path;

    #[test]
    fn test_parses_example() {
        let reader = BufReader::new(
            File::open(Path::new("/home/zjn/git/spectrum-impl/experiments.json"))
                .expect("Cannot find experiments.json file."),
        );

        // The file is JSON *with comments*, so not valid JSON.
        // We need to strip comments out before we parse it.
        let mut json = String::new();
        for line in reader.lines() {
            let line = line.unwrap();
            // Only matches comments at beginning of line, but that's okay.
            // We don't want to write a full parser.
            if line.trim_start_matches(' ').starts_with("//") {
                continue;
            }
            json += &line;
        }

        let _experiments: Vec<Experiment> =
            serde_json::from_str(&json).expect("Serialization should have been successful.");
    }
}
