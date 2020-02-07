use crate::{
    config,
    services::{Group, LeaderInfo, PublisherInfo, Service, WorkerInfo},
};

use config::store::{Error, Key, Store};
use std::net::SocketAddr;

fn to_config_key(service: Service) -> Key {
    match service {
        Service::Leader(info) => vec![
            "nodes".to_string(),
            "groups".to_string(),
            info.group.idx.to_string(),
            "leader".to_string(),
        ],
        Service::Publisher(_) => vec!["nodes".to_string(), "publisher".to_string()],
        Service::Worker(info) => vec![
            "nodes".to_string(),
            "groups".to_string(),
            info.group.idx.to_string(),
            info.idx.to_string(),
        ],
        Service::Client(_) => {
            panic!("Clients are not stored in the config registry.");
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Node {
    pub service: Service,
    pub addr: SocketAddr,
}

impl Node {
    pub fn new(service: Service, addr: SocketAddr) -> Node {
        Node { service, addr }
    }
}

/// Register a server of the given type at the given address.
pub async fn register<C: Store>(config: &C, node: Node) -> Result<(), Error> {
    config
        .put(to_config_key(node.service), node.addr.to_string())
        .await
}

pub async fn resolve_all<C: Store>(config: &C) -> Result<Vec<Node>, Error> {
    Ok(config
        .list(vec!["nodes".to_string()])
        .await?
        .iter()
        .map(|(key, addr)| {
            let key: Vec<&str> = key.iter().map(|x| x.as_str()).collect();
            // TODO(zjn): don't unwrap
            let service = match key[..] {
                ["nodes", "groups", group, "leader"] => {
                    LeaderInfo::new(Group::new(group.parse().unwrap())).into()
                }
                ["nodes", "groups", group, idx] => {
                    WorkerInfo::new(Group::new(group.parse().unwrap()), idx.parse().unwrap()).into()
                }
                ["nodes", "publisher"] => PublisherInfo::new().into(),
                _ => {
                    panic!(); // TODO(zjn): better error
                }
            };
            Node::new(service, addr.parse().unwrap())
        })
        .collect())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{config, net::tests::addrs};
    use config::tests::inmem_stores;
    use futures::executor::block_on;
    use prop::collection::hash_map;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::iter::FromIterator;

    pub fn services() -> impl Strategy<Value = Service> {
        prop_oneof![
            Just(PublisherInfo::new()).prop_map(Service::from),
            any::<u16>()
                .prop_map(Group::new)
                .prop_map(LeaderInfo::new)
                .prop_map(Service::from),
            (any::<u16>(), any::<u16>())
                .prop_map(|(group, idx)| WorkerInfo::new(Group::new(group), idx))
                .prop_map(Service::from),
        ]
    }

    fn node_sets() -> impl Strategy<Value = HashSet<Node>> {
        hash_map(services(), addrs(), ..100).prop_map(|services_to_addrs| {
            HashSet::from_iter(
                services_to_addrs
                    .into_iter()
                    .map(|(service, addr)| Node::new(service, addr)),
            )
        })
    }

    proptest! {
        #[test]
        fn test_register_and_resolve(store in inmem_stores(), nodes in node_sets()) {
            let work = async {
                for node in &nodes {
                    register(&store, node.clone()).await.unwrap();
                }

                let actual = HashSet::from_iter(
                    resolve_all(&store).await.unwrap().into_iter()
                );

                assert_eq!(actual, nodes);
            };
            block_on(work);
        }
    }
}
