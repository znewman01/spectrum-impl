#![allow(clippy::unit_arg)] // weird cargo clippy bug; complains about "derive(Arbitrary)"

use crate::config::store::{Error, Store};
use crate::services::{
    discovery::Node, ClientInfo, Group, LeaderInfo, PublisherInfo, Service, WorkerInfo,
};

use serde::{Deserialize, Serialize};
use std::iter::{once, IntoIterator};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Experiment {
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    pub groups: u16,
    workers_per_group: u16,
    pub clients: u16,
    pub channels: usize,
}

impl Experiment {
    pub fn new(groups: u16, workers_per_group: u16, clients: u16, channels: usize) -> Experiment {
        assert!(groups >= 1, "Expected at least 1 group.");
        assert!(
            workers_per_group >= 1,
            "Expected at least 1 worker per group."
        );
        assert!(clients >= 1, "Expected at least 1 client.");
        Experiment {
            groups,
            workers_per_group,
            clients,
            channels,
        }
    }

    pub fn iter_services(self) -> impl Iterator<Item = Service> {
        let publishers = once((PublisherInfo::new()).into());
        let groups = (0..self.groups).map(Group::new);
        let leaders = groups.clone().map(LeaderInfo::new).map(Service::from);
        let workers = groups.flat_map(move |group| {
            (0..self.workers_per_group).map(move |idx| (WorkerInfo::new(group, idx)).into())
        });

        publishers.chain(leaders).chain(workers)
    }

    // TODO(zjn): combine with iter_services
    pub fn iter_clients(self) -> impl Iterator<Item = Service> {
        (0..self.clients).map(ClientInfo::new).map(Service::from)
    }
}

// Get the peer nodes for a worker.
//
// These should be all worker nodes in the same group except the worker itself.
#[allow(dead_code)]
pub fn filter_peers<I>(info: WorkerInfo, all_nodes: I) -> Vec<Node>
where
    I: IntoIterator<Item = Node>,
{
    let my_info = info;
    all_nodes
        .into_iter()
        .filter(|node| {
            if let Service::Worker(their_info) = node.service {
                their_info != my_info && their_info.group == my_info.group
            } else {
                false
            }
        })
        .collect()
}

pub async fn write_to_store<C: Store>(config: &C, experiment: Experiment) -> Result<(), Error> {
    let json_str =
        serde_json::to_string(&experiment).map_err(|err| Error::new(&err.to_string()))?;
    config
        .put(
            vec!["experiment".to_string(), "config".to_string()],
            json_str,
        )
        .await?;
    Ok(())
}

pub async fn read_from_store<C: Store>(config: &C) -> Result<Experiment, Error> {
    let json_str = config
        .get(vec!["experiment".to_string(), "config".to_string()])
        .await?
        .ok_or_else(|| Error::new("No experiment string in store."))?;
    Ok(serde_json::from_str(&json_str).map_err(|err| Error::new(&err.to_string()))?)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::config::tests::inmem_stores;
    use core::ops::Range;
    use futures::executor::block_on;
    use proptest::prelude::*;

    pub fn experiments() -> impl Strategy<Value = Experiment> {
        let groups: Range<u16> = 1..10;
        let workers_per_group: Range<u16> = 1..10;
        let clients: Range<u16> = 1..10;
        let channels: Range<usize> = 1..10;
        (groups, workers_per_group, clients, channels)
            .prop_map(|(g, w, cl, ch)| Experiment::new(g, w, cl, ch))
    }

    proptest! {
        #[test]
        fn test_experiment_roundtrip(config in inmem_stores(), experiment in experiments()) {
            block_on(async {
                write_to_store(&config, experiment).await.unwrap();
                assert_eq!(
                    read_from_store(&config).await.unwrap(),
                    experiment);
            });
        }

        #[test]
        fn test_experiment_iter_services(experiment in experiments()) {
            let services: Vec<Service> = experiment.iter_services().collect();

            let mut publishers = vec![];
            let mut leaders = vec![];
            let mut workers = vec![];
            for service in services {
                match service {
                    Service::Publisher(_) => { publishers.push(service) },
                    Service::Leader(_) => { leaders.push(service) },
                    Service::Worker(_) => { workers.push(service) },
                    Service::Client(_) => {
                        panic!("Clients not (yet) in iter_services");
                    }
                }
            }
            let actual = (publishers.len(), leaders.len(), workers.len());
            let expected = (1,
                            experiment.groups as usize,
                            (experiment.groups * experiment.workers_per_group) as usize);
            prop_assert_eq!(actual, expected);
        }

        #[test]
        fn test_experiment_iter_clients(experiment in experiments()) {
            let clients: Vec<Service> = experiment.iter_clients().collect();

            for client in &clients {
                match client {
                    Service::Client(_) => {}
                    _ => { panic!("Only clients expected in iter_clients()."); }
                }
            }

            prop_assert_eq!(clients.len(), experiment.clients as usize);
        }

        #[test]
        fn test_filter_peers(experiment in experiments()) {
            let services: Vec<Service> = experiment.iter_services().collect();
            let nodes = services.iter().map(|&service| Node::new(service, "127.0.0.1:22".parse().unwrap()));

            let info = *services
                .iter()
                .filter_map(|service| match service {
                    Service::Worker(info) => Some(info),
                    _ => None
                })
                .next()
                .expect("Should have at least one worker.");

            let peers = filter_peers(info, nodes);

            for peer in &peers {
                if let Service::Worker(other_info) = peer.service {
                    assert_ne!(other_info, info,
                               "A node is not a peer of itself.");
                    assert_eq!(other_info.group, info.group,
                               "A peer must be in the same group.");
                } else {
                    panic!("Peers must be Workers.");
                }
            }
            assert_eq!(peers.len(), (experiment.workers_per_group - 1) as usize,
                       "All workers in the group except the node itself should be peers.");
        }
    }
}
