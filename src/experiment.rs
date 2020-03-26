use crate::config::store::{Error, Store};
use crate::protocols::{
    insecure::{self, InsecureProtocol},
    secure::{self, SecureProtocol},
    wrapper::{ChannelKeyWrapper, ProtocolWrapper},
};
use crate::services::{ClientInfo, Group, LeaderInfo, PublisherInfo, Service, WorkerInfo};

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::iter::{once, IntoIterator};

const MSG_SIZE: usize = 125_000; // 1 megabit in bytes

// TODO: properly serialize protocol details
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Experiment {
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    groups: u16,
    group_size: u16,
    clients: u16, // TODO(zjn): make u32
    channels: usize,
    secure: bool,
}

impl Experiment {
    pub fn new(groups: u16, group_size: u16, clients: u16, channels: usize) -> Experiment {
        assert!(groups >= 1, "Expected at least 1 group.");
        assert!(group_size >= 1, "Expected at least 1 worker per group.");
        assert!(clients >= 1, "Expected at least 1 client.");
        assert!(
            clients as usize >= channels,
            "Expected at least as many clients as channels."
        );
        let secure = false;
        if secure {
            assert_eq!(groups, 2, "Secure protocol only implemented for 2 groups.");
        }
        Experiment {
            groups,
            group_size,
            clients,
            channels,
            secure,
        }
    }

    pub fn groups(&self) -> u16 {
        self.groups
    }

    pub fn group_size(&self) -> u16 {
        self.group_size
    }

    pub fn clients(&self) -> u16 {
        self.clients
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn iter_services(self) -> impl Iterator<Item = Service> {
        let publishers = once((PublisherInfo::new()).into());
        let groups = (0..self.groups).map(Group::new);
        let leaders = groups.clone().map(LeaderInfo::new).map(Service::from);
        let workers = groups.flat_map(move |group| {
            (0..self.group_size()).map(move |idx| (WorkerInfo::new(group, idx)).into())
        });

        publishers.chain(leaders).chain(workers)
    }

    // TODO(zjn): combine with iter_services
    pub fn iter_clients(self) -> impl Iterator<Item = Service> {
        let viewers = (0..(self.channels() as u16))
            .zip(self.get_keys().into_iter())
            .map(|(idx, key)| {
                let msg = (1usize..MSG_SIZE)
                    .map(|x| (x % 256).try_into().unwrap())
                    .chain(once((idx as u8) + 100))
                    .collect::<Vec<_>>()
                    .into();
                ClientInfo::new_broadcaster(idx, msg, key)
            })
            .map(Service::from);
        let broadcasters = ((self.channels() as u16)..self.clients())
            .map(ClientInfo::new)
            .map(Service::from);
        viewers.chain(broadcasters)
    }

    pub fn get_protocol(&self) -> ProtocolWrapper {
        let groups = self.groups().try_into().unwrap();
        if self.secure {
            let protocol = SecureProtocol::with_aes_prg_dpf(40, groups, MSG_SIZE);
            ProtocolWrapper::Secure(protocol)
        } else {
            let protocol = InsecureProtocol::new(groups, self.channels(), MSG_SIZE);
            ProtocolWrapper::Insecure(protocol)
        }
    }

    pub fn get_keys(&self) -> Vec<ChannelKeyWrapper> {
        if self.secure {
            let protocol =
                SecureProtocol::with_aes_prg_dpf(40, self.groups().try_into().unwrap(), MSG_SIZE);
            let field = protocol.vdpf.field;
            (0..self.channels())
                .map(|idx| {
                    secure::ChannelKey::<secure::ConcreteVdpf>::new(
                        idx,
                        field.new_element(idx.into()),
                    )
                    .into()
                })
                .collect()
        } else {
            (0..self.channels())
                .map(|idx| insecure::ChannelKey::new(idx, format!("password{}", idx)).into())
                .collect()
        }
    }
}

// Get the peer nodes for a worker.
//
// These should be all worker nodes in the same group except the worker itself.
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
                            experiment.groups() as usize,
                            (experiment.groups() * experiment.group_size()) as usize);
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

            prop_assert_eq!(clients.len(), experiment.clients() as usize);
        }
    }
}
