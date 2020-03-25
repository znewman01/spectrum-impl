use crate::config::store::{Error, Store};
use crate::protocols::{
    insecure::{self, InsecureProtocol},
    secure::{self, SecureProtocol},
    wrapper::{ChannelKeyWrapper, ProtocolWrapper},
};
use crate::services::{
    discovery::Node, ClientInfo, Group, LeaderInfo, PublisherInfo, Service, WorkerInfo,
};

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::iter::{once, IntoIterator};

const MSG_SIZE: usize = 100;

// TODO: properly serialize protocol details
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Experiment {
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    pub groups: u16,
    pub workers_per_group: u16,
    pub clients: u16,
    pub channels: usize,
    pub secure: bool,
}

impl Experiment {
    pub fn new(groups: u16, workers_per_group: u16, clients: u16, channels: usize) -> Experiment {
        assert!(groups >= 1, "Expected at least 1 group.");
        assert!(
            workers_per_group >= 1,
            "Expected at least 1 worker per group."
        );
        assert!(clients >= 1, "Expected at least 1 client.");
        assert!(
            clients as usize >= channels,
            "Expected at least as many clients as channels."
        );
        let secure = false;
        if secure {
            assert_eq!(
                channels, 2,
                "Secure protocol only implemented for 2 channels."
            );
        }
        Experiment {
            groups,
            workers_per_group,
            clients,
            channels,
            secure,
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
        let viewers = (0..(self.channels as u16))
            .zip(self.get_keys().into_iter())
            .map(|(idx, key)| {
                let msg = (1u8..MSG_SIZE.try_into().unwrap())
                    .chain(once((idx as u8) + 100))
                    .collect::<Vec<_>>()
                    .into();
                ClientInfo::new_broadcaster(idx, msg, key)
            })
            .map(Service::from);
        let broadcasters = ((self.channels as u16)..self.clients)
            .map(ClientInfo::new)
            .map(Service::from);
        viewers.chain(broadcasters)
    }

    pub fn get_protocol(&self) -> Box<dyn ProtocolWrapper + Sync + Send> {
        if self.secure {
            Box::new(SecureProtocol::with_aes_prg_dpf(
                40,
                self.groups.try_into().unwrap(),
                MSG_SIZE,
            ))
        } else {
            Box::new(InsecureProtocol::new(
                self.groups.try_into().unwrap(),
                self.channels,
                MSG_SIZE,
            ))
        }
    }

    pub fn get_keys(&self) -> Vec<ChannelKeyWrapper> {
        if self.secure {
            let protocol =
                SecureProtocol::with_aes_prg_dpf(40, self.groups.try_into().unwrap(), MSG_SIZE);
            let field = protocol.vdpf.field;
            (0..self.channels)
                .map(|idx| {
                    secure::ChannelKey::<secure::ConcreteVdpf>::new(
                        idx,
                        field.new_element(idx.into()),
                    )
                    .into()
                })
                .collect()
        } else {
            (0..self.channels)
                .map(|idx| insecure::ChannelKey::new(idx, format!("password{}", idx)).into())
                .collect()
        }
    }
}

// Get the peer nodes for a worker.
//
// These should be all worker nodes in the same group except the worker itself.
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
        let channels: Range<usize> = 1..10;
        (groups, workers_per_group, channels).prop_flat_map(|(g, w, ch)| {
            let clients: Range<u16> = (ch as u16)..10;
            clients.prop_map(move |cl| Experiment::new(g, w, cl, ch))
        })
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
            let nodes = services.iter().map(|service| Node::new(service.clone(), "127.0.0.1:22".parse().unwrap()));

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
