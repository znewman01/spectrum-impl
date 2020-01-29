/// Service discovery
use crate::config;

use chrono::prelude::*;
use config::store::{Error, Key, Store};
#[cfg(test)] // TODO(zjn): move to mod tests (https://github.com/AltSysrq/proptest/pull/106)
use proptest_derive::Arbitrary;
use std::time::Duration;
use tokio::time::delay_for;

// TODO(zjn): make configurable. Short for local testing; long for real deployments
const RETRY_DELAY: Duration = Duration::from_millis(100);
const RETRY_ATTEMPTS: usize = 1000;

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

pub async fn get_addrs<C: Store>(config: &C, service: ServiceType) -> Result<Vec<String>, Error> {
    Ok(config
        .list(service.to_config_key())
        .await?
        .iter()
        .map(|(k, _v)| k.last().unwrap().to_string())
        .collect())
}

async fn get_start_time<C: Store>(config: &C) -> Result<Option<DateTime<FixedOffset>>, Error> {
    let key = vec!["experiment".to_string(), "start-time".to_string()];
    let start_time_str: Option<String> = config.get(key).await?;
    start_time_str
        .map(|s| DateTime::parse_from_rfc3339(&s).map_err(|err| Error::new(&err.to_string())))
        .transpose()
}

pub async fn set_quorum<C: Store>(config: &C, dt: DateTime<FixedOffset>) -> Result<(), Error> {
    let key = vec!["experiment".to_string(), "start-time".to_string()];
    config.put(key, dt.to_rfc3339()).await?;
    Ok(())
}

async fn wait_for_quorum_helper<C: Store>(
    config: C,
    delay: Duration,
    attempts: usize,
) -> Result<DateTime<FixedOffset>, Error> {
    for _ in 0..attempts {
        println!("checking");
        match get_start_time(&config).await? {
            Some(start_time) => {
                return Ok(start_time);
            }
            None => {
                delay_for(delay).await;
            }
        };
    }
    let msg = format!("Quorum not reached after {} attempts", attempts);
    Err(Error::new(&msg))
}

pub async fn wait_for_quorum<C: Store>(config: C) -> Result<DateTime<FixedOffset>, Error> {
    wait_for_quorum_helper(config, RETRY_DELAY, RETRY_ATTEMPTS).await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config;
    use config::tests::{inmem_stores, KEY};
    use futures::executor::block_on;
    use futures::FutureExt;
    use prop::collection::{hash_map, hash_set};
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};
    use tokio::sync::oneshot::{channel, error::TryRecvError};
    use tokio::task::yield_now;

    #[tokio::test(threaded_scheduler)]
    async fn test_wait_for_quorum() {
        let store = config::factory::from_string("").expect("Failed to create store.");
        let (tx, mut rx) = channel();
        let handle = tokio::spawn(
            wait_for_quorum_helper(store.clone(), Duration::from_millis(0), 2)
                .inspect(|_| tx.send(()).unwrap()),
        );
        yield_now().await;

        assert_eq!(
            rx.try_recv().expect_err("Task not have completed yet."),
            TryRecvError::Empty
        );

        let dt = DateTime::<FixedOffset>::from(Utc::now());
        set_quorum(&store, dt).await.unwrap();

        let result = handle
            .await
            .expect("Task should have completed without crashing.");
        result.expect("Task should have been successsful.");
    }

    #[tokio::test]
    async fn test_wait_for_quorum_failure() {
        let store = config::factory::from_string("").expect("Failed to create store.");
        let result = wait_for_quorum_helper(store, Duration::from_millis(0), 1).await;
        result.expect_err("Expected failure, as quorum never reached.");
    }

    fn service_entries() -> impl Strategy<Value = HashMap<ServiceType, HashSet<String>>> {
        hash_map(any::<ServiceType>(), hash_set(KEY, 0..10), 0..3)
    }

    proptest! {
        #[test]
        fn test_register_service(store in inmem_stores(), service_entries in service_entries()) {
            let work = async {
                for (&service, addrs) in &service_entries {
                    for addr in addrs {
                        register(&store, service, &addr).await.unwrap();
                    }
                }

                let mut actual = HashMap::<ServiceType, HashSet<String>>::new();
                for &service in service_entries.keys() {
                    let mut addrs = HashSet::new();
                    for addr in get_addrs(&store, service).await.unwrap() {
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
