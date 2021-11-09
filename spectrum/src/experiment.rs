use crate::config::store::{Error, Store};
use crate::protocols::wrapper::{ChannelKeyWrapper, ProtocolWrapper};
use crate::services::{ClientInfo, Group, LeaderInfo, PublisherInfo, Service, WorkerInfo};

use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::iter::{once, IntoIterator};

// TODO: properly serialize protocol details
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Experiment {
    protocol: ProtocolWrapper,
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    group_size: u16,
    clients: u128,
    pub hammer: bool,
    keys: Vec<ChannelKeyWrapper>,
}

impl Experiment {
    pub fn new(
        protocol: ProtocolWrapper,
        group_size: u16,
        clients: u128,
        hammer: bool,
        keys: Vec<ChannelKeyWrapper>,
    ) -> Experiment {
        assert!(group_size >= 1, "Expected at least 1 worker per group.");
        assert!(clients >= 1, "Expected at least 1 client.");
        assert_eq!(protocol.num_channels(), keys.len());
        Experiment {
            protocol,
            group_size,
            clients,
            hammer,
            keys,
        }
    }

    pub fn new_sample_keys(
        protocol: ProtocolWrapper,
        group_size: u16,
        clients: u128,
        hammer: bool,
    ) -> Self {
        use spectrum_primitives::{AuthKey, Sampleable, TwoKeyPubAuthKey};
        let channels = 0..protocol.num_channels();
        let keys: Vec<ChannelKeyWrapper> = match &protocol {
            ProtocolWrapper::Secure(_) => {
                channels.map(|_: usize| AuthKey::sample().into()).collect()
            }
            ProtocolWrapper::SecurePub(_) => channels
                .map(|_: usize| TwoKeyPubAuthKey::sample().into())
                .collect(),
            ProtocolWrapper::SecureMultiKey(_) => {
                channels.map(|_: usize| AuthKey::sample().into()).collect()
            }
        };
        Experiment::new(protocol, group_size, clients, hammer, keys)
    }

    pub fn groups(&self) -> u16 {
        self.protocol.num_parties().try_into().unwrap()
    }

    pub fn group_size(&self) -> u16 {
        self.group_size
    }

    pub fn clients(&self) -> u128 {
        self.clients
    }

    pub fn channels(&self) -> usize {
        self.protocol.num_channels()
    }

    pub fn msg_size(&self) -> usize {
        self.protocol.message_len()
    }

    pub fn iter_services(&self) -> impl Iterator<Item = Service> + '_ {
        let publishers = once((PublisherInfo::new()).into());
        let groups = (0..self.groups()).map(Group::new);
        let workers = groups.clone().flat_map(move |group| {
            (0..self.group_size()).map(move |idx| (WorkerInfo::new(group, idx)).into())
        });

        let iter = publishers.chain(workers);

        if self.hammer {
            return Box::new(iter) as Box<dyn Iterator<Item = Service>>;
        }

        let leaders = groups.map(LeaderInfo::new).map(Service::from);
        Box::new(iter.chain(leaders)) as Box<dyn Iterator<Item = Service>>
    }

    // TODO(zjn): combine with iter_services
    pub fn iter_clients(&self) -> impl Iterator<Item = Service> + '_ {
        let msg_size = self.msg_size();
        let viewers = (0..(self.channels() as u128))
            .zip(self.get_keys().into_iter())
            .map(move |(idx, key)| {
                let msg: Vec<u8> = match self.get_protocol() {
                    ProtocolWrapper::SecureMultiKey(_) => {
                        // need multiple of 32
                        let rem = msg_size % 32;
                        let size = if rem != 0 {
                            msg_size + (32 - rem)
                        } else {
                            msg_size
                        };
                        let chunks = size / 32;
                        use std::iter::repeat;
                        let good_elem: Vec<u8> = vec![
                            203, 85, 12, 213, 56, 234, 12, 193, 19, 132, 128, 64, 142, 110, 170,
                            185, 179, 108, 97, 63, 13, 211, 247, 120, 79, 219, 110, 234, 131, 123,
                            19, 215,
                        ];
                        repeat(good_elem).take(chunks).flatten().collect()
                    }
                    _ => {
                        vec![(idx % 256).try_into().unwrap(); msg_size]
                    }
                };
                ClientInfo::new_broadcaster(idx, msg.into(), key)
            })
            .map(Service::from);
        let broadcasters = ((self.channels() as u128)..self.clients())
            .map(ClientInfo::new)
            .map(Service::from);
        viewers.chain(broadcasters)
    }

    pub fn get_protocol(&self) -> &ProtocolWrapper {
        &self.protocol
    }

    pub fn get_keys(&self) -> Vec<ChannelKeyWrapper> {
        self.keys.clone()
    }
}

// Get the peer nodes for a worker.
//
// These should be all worker nodes in the same group except the worker itself.
pub async fn write_to_store<C: Store>(config: &C, experiment: &Experiment) -> Result<(), Error> {
    let json_str = serde_json::to_string(experiment).map_err(|err| Error::new(&err.to_string()))?;
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

    // TODO: restore
    // impl Arbitrary for Experiment {
    //     type Parameters = bool;
    //     type Strategy = BoxedStrategy<Self>;

    //     fn arbitrary_with(hammer: bool) -> Self::Strategy {
    //         let group_size: Range<u16> = 1..10;
    //         // TODO(zjn): add SecureProtocols too (maybe via impl arbitrary for ProtocolWrapper?)
    //         let protocols = any::<insecure::InsecureProtocol>().prop_map(ProtocolWrapper::from);
    //         (protocols, group_size)
    //             .prop_flat_map(move |(protocol, group_size)| {
    //                 let clients: Range<u128> = (protocol.num_channels() as u128)..20;
    //                 let keys =
    //                     prop::collection::vec(any::<ChannelKeyWrapper>(), protocol.num_channels());
    //                 (clients, keys).prop_map(move |(clients, keys)| {
    //                     Experiment::new(protocol.clone(), group_size, clients, hammer, keys)
    //                 })
    //             })
    //             .boxed()
    //     }
    // }

    // proptest! {
    //     #[test]
    //     fn test_experiment_roundtrip(config in inmem_stores(), experiment: Experiment) {
    //         block_on(async {
    //             write_to_store(&config, &experiment).await.unwrap();
    //             assert_eq!(
    //                 read_from_store(&config).await.unwrap(),
    //                 experiment);
    //         });
    //     }

    //     #[test]
    //     fn test_experiment_iter_services(experiment: Experiment) {
    //         let services: Vec<Service> = experiment.iter_services().collect();

    //         let mut publishers = vec![];
    //         let mut leaders = vec![];
    //         let mut workers = vec![];
    //         for service in services {
    //             match service {
    //                 Service::Publisher(_) => { publishers.push(service) },
    //                 Service::Leader(_) => { leaders.push(service) },
    //                 Service::Worker(_) => { workers.push(service) },
    //                 Service::Client(_) => {
    //                     panic!("Clients not (yet) in iter_services");
    //                 }
    //             }
    //         }
    //         let actual = (publishers.len(), leaders.len(), workers.len());
    //         let expected = (1,
    //                         experiment.groups() as usize,
    //                         (experiment.groups() * experiment.group_size()) as usize);
    //         prop_assert_eq!(actual, expected);
    //     }

    //     #[test]
    //     fn test_experiment_iter_services_hammer(experiment in Experiment::arbitrary_with(true)) {
    //         let services: Vec<Service> = experiment.iter_services().collect();

    //         let mut publishers = vec![];
    //         let mut workers = vec![];
    //         for service in services {
    //             match service {
    //                 Service::Publisher(_) => { publishers.push(service) },
    //                 Service::Leader(_) => { return Err(TestCaseError::fail("Didn't expect leaders in hammer mode")); },
    //                 Service::Worker(_) => { workers.push(service) },
    //                 Service::Client(_) => {
    //                     panic!("Clients not (yet) in iter_services");
    //                 }
    //             }
    //         }
    //         let expected_workers = (experiment.groups() * experiment.group_size()) as usize;
    //         prop_assert_eq!(workers.len(), expected_workers);
    //         prop_assert_eq!(publishers.len(), 1);
    //     }

    //     #[test]
    //     fn test_experiment_iter_clients(experiment: Experiment) {
    //         let clients: Vec<Service> = experiment.iter_clients().collect();

    //         for client in &clients {
    //             match client {
    //                 Service::Client(_) => {}
    //                 _ => { panic!("Only clients expected in iter_clients()."); }
    //             }
    //         }

    //         prop_assert_eq!(clients.len(), experiment.clients() as usize);
    //     }
    // }
}
