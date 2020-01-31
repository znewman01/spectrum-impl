use crate::config;

use config::store::{Error, Key, Store};

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Group(pub u16);

// TODO(zjn): make these store group/idx info in them
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum Service {
    Leader { group: Group },
    Publisher,
    Worker { group: Group, idx: u16 },
}

impl Service {
    fn to_config_key(self) -> Key {
        match self {
            Service::Leader { group } => vec![
                "nodes".to_string(),
                "groups".to_string(),
                group.0.to_string(),
                "leader".to_string(),
            ],
            Service::Publisher => vec!["nodes".to_string(), "publisher".to_string()],
            Service::Worker { group, idx } => vec![
                "nodes".to_string(),
                "groups".to_string(),
                group.0.to_string(),
                idx.to_string(),
            ],
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Node {
    pub service: Service,
    pub addr: String, // TODO(zjn): make a SocketAddr
}

impl Node {
    pub fn new(service: Service, addr: String) -> Node {
        Node { service, addr }
    }
}

/// Register a server of the given type at the given address.
pub async fn register<C: Store>(config: &C, node: Node) -> Result<(), Error> {
    config
        .put(node.service.to_config_key(), node.addr.to_string())
        .await
}

pub async fn resolve_all<C: Store>(config: &C) -> Result<Vec<Node>, Error> {
    Ok(config
        .list(vec!["nodes".to_string()])
        .await?
        .iter()
        .map(|(key, addr)| {
            let key: Vec<&str> = key.iter().map(|x| x.as_str()).collect();
            let service = match key[..] {
                ["nodes", "groups", group, "leader"] => Service::Leader {
                    group: Group(group.parse().unwrap()),
                },
                ["nodes", "groups", group, idx] => {
                    Service::Worker {
                        group: Group(group.parse().unwrap()),
                        idx: idx.parse().unwrap(), // TODO(zjn): don't unwrap
                    }
                }
                ["nodes", "publisher"] => Service::Publisher,
                _ => {
                    panic!(); // TODO(zjn): better error
                }
            };
            Node::new(service, addr.to_string())
        })
        .collect())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config;
    use config::tests::inmem_stores;
    use futures::executor::block_on;
    use prop::collection::hash_set;
    use prop::strategy::Union;
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::iter::FromIterator;

    fn services() -> impl Strategy<Value = Service> {
        Union::new(vec![Just(Service::Publisher)].into_iter())
    }

    fn nodes() -> impl Strategy<Value = Node> {
        services().prop_map(|service| Node::new(service, "".to_string()))
    }

    proptest! {
        #[test]
        fn test_register_and_resolve(store in inmem_stores(), nodes in hash_set(nodes(), 0..100)) {
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
