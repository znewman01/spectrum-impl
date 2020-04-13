///! Module containing experiment infrastructure.
///!
///! To submit
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::convert::TryInto;
use std::time::Duration;

// TODO: this can all go away when serde has support for default literals.
// https://github.com/serde-rs/serde/issues/368

fn ami_0fc20dd1da406780b() -> String {
    "ami-0fc20dd1da406780b".to_string()
}

fn _250() -> u16 {
    250
}

fn _1() -> u16 {
    1
}

fn _8() -> u16 {
    8
}

fn _16() -> u16 {
    16
}

fn m5_xlarge() -> MachineType {
    MachineType {
        instance_type: "m5.xlarge".to_string(),
    }
}

fn c5_24xlarge() -> MachineType {
    MachineType {
        instance_type: "c5.24xlarge".to_string(),
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
#[serde(transparent)]
pub struct MachineType {
    pub instance_type: String,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
pub struct MachineTypeConfiguration {
    /// Machine type of the publisher/etcd machine
    #[serde(default = "m5_xlarge")]
    pub publisher: MachineType,

    /// Machine type of the server for running workers/leaders.
    #[serde(default = "c5_24xlarge")]
    pub worker: MachineType,

    /// Machine type of the server for simulating clients.
    #[serde(default = "m5_xlarge")]
    pub client: MachineType,
}

impl Default for MachineTypeConfiguration {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

/// Configuration for the environment in which to run an experiment.
///
/// As a rule of thumb, anything that can requires a change in infrastructure
/// (machines set-up/torn-down) should go here.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Environment {
    /// The AWS instance types to use for each type of VM in the experiment.
    pub machine_types: MachineTypeConfiguration,

    /// Total number of client machines
    pub client_machines: u16,

    /// Total number of worker machines
    pub worker_machines: u16,

    /// Number of worker process to run on each machine
    /// (Needs to be part of the environment as it relates to config setup).
    pub workers_per_machine: u16,

    /// Amazon AMI ID to use as the base image for experiments.
    ///
    /// The default is a recent (as of 2020-03-30) build of Ubuntu server 18.04.
    pub base_ami: String,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
pub enum Protocol {
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

impl Default for Protocol {
    fn default() -> Self {
        Self::Symmetric { security: 16 }
    }
}

impl Protocol {
    pub fn groups(&self) -> u16 {
        match self {
            Self::Symmetric { .. } => 2,
            Self::Insecure { parties } => *parties,
            Self::SeedHomomorphic { parties } => *parties,
        }
    }
}

/// A configuration for an experiment to run.
#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    /// The AWS instance types to use for each type of VM in the experiment.
    #[serde(default)]
    pub machine_types: MachineTypeConfiguration,

    /// Maximum number of clients a given machine should simulate
    #[serde(default = "_250")]
    pub clients_per_machine: u16,

    /// Amazon AMI ID to use as the base image for experiments.
    ///
    /// The default is a recent (as of 2020-03-30) build of Ubuntu server 18.04.
    #[serde(default = "ami_0fc20dd1da406780b")]
    pub base_ami: String,

    // TODO: AWS region.
    /// Number of worker processes to run on a given machine
    #[serde(default = "_8")]
    pub workers_per_machine: u16,

    /// Number of worker machines per group.
    #[serde(default = "_1")]
    pub worker_machines_per_group: u16,

    // TODO(zjn): move the following to Experiment when we can change them at runtime.
    /// Number of clients to simulate.
    pub clients: u32,

    /// Number of channels for the protocol.
    pub channels: u16,

    // The size, in bytes, for each message.
    pub message_size: u32,

    // The protocol to run.
    //
    // Note that this encapsulates the number of trust groups.
    #[serde(default)]
    pub protocol: Protocol,
}

impl Experiment {
    /// Returns all AWS instance types for all VM types in this experiment.
    pub fn instance_types(&self) -> Vec<String> {
        let machines = &self.machine_types;
        vec![
            machines.publisher.instance_type.clone(),
            machines.worker.instance_type.clone(),
            machines.client.instance_type.clone(),
        ]
    }

    pub fn by_environment(experiments: Vec<Experiment>) -> HashMap<Environment, Vec<Experiment>> {
        experiments
            .into_iter()
            .map(|e| (e.environment(), e))
            .into_group_map()
    }

    pub fn groups(&self) -> u16 {
        self.protocol.groups()
    }

    // Total number of worker processes
    pub fn group_size(&self) -> u16 {
        self.worker_machines_per_group * self.workers_per_machine
    }

    fn environment(&self) -> Environment {
        // The last machine may have fewer than clients_per_machine clients
        let client_machines = ((self.clients - 1) / (self.clients_per_machine as u32) + 1)
            .try_into()
            .unwrap();
        Environment {
            client_machines,
            machine_types: self.machine_types.clone(),
            worker_machines: self.worker_machines_per_group * self.groups(),
            workers_per_machine: self.workers_per_machine,
            base_ami: self.base_ami.clone(),
        }
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

    #[test]
    fn test_parses_example() {
        let json = include_str!("data/test.json");
        let _experiments: Vec<Experiment> =
            serde_json::from_str(&json).expect("Serialization should have been successful.");
    }
}
