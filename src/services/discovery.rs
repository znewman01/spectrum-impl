use crate::config;

use config::store::{Error, Key, Store};
#[cfg(test)] // TODO(zjn): move to mod tests (https://github.com/AltSysrq/proptest/pull/106)
use proptest_derive::Arbitrary;

// TODO(zjn): make these store group/idx info in them
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum ServiceType {
    #[allow(dead_code)]
    Leader,
    #[allow(dead_code)]
    Publisher,
    Worker,
}

impl ServiceType {
    fn to_config_key(self) -> Key {
        let key = match self {
            ServiceType::Leader => "leader",
            ServiceType::Publisher => "publisher",
            ServiceType::Worker => "worker",
        };
        vec![key.to_string()]
    }
}

/// Register a server of the given type at the given address.
pub async fn register<C: Store>(config: &C, service: ServiceType, addr: &str) -> Result<(), Error> {
    // TODO(zjn): verify not already registered
    let mut prefix = service.to_config_key();
    prefix.push(addr.to_string());
    config.put(prefix, "".to_string()).await
}

pub async fn resolve_all<C: Store>(config: &C, service: ServiceType) -> Result<Vec<String>, Error> {
    Ok(config
        .list(service.to_config_key())
        .await?
        .iter()
        .map(|(k, _v)| k.last().unwrap().to_string())
        .collect())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config;
    use config::tests::{inmem_stores, KEY};
    use futures::executor::block_on;
    use prop::collection::{hash_map, hash_set};
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};

    fn service_entries() -> impl Strategy<Value = HashMap<ServiceType, HashSet<String>>> {
        hash_map(any::<ServiceType>(), hash_set(KEY, 0..10), 0..3)
    }

    proptest! {
        #[test]
        fn test_register_and_resolve(store in inmem_stores(), service_entries in service_entries()) {
            let work = async {
                for (&service, addrs) in &service_entries {
                    for addr in addrs {
                        register(&store, service, &addr).await.unwrap();
                    }
                }

                let mut actual = HashMap::<ServiceType, HashSet<String>>::new();
                for &service in service_entries.keys() {
                    let mut addrs = HashSet::new();
                    for addr in resolve_all(&store, service).await.unwrap() {
                        addrs.insert(addr);
                    }
                    actual.insert(service, addrs);
                }
                assert_eq!(actual, service_entries);
            };
            block_on(work);
        }
    }
}
