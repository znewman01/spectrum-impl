use crate::{
    config::store::{Error, Store},
    experiment::Experiment,
    services::{
        discovery::{resolve_all, Service},
        retry::error_policy,
    },
};

use chrono::prelude::*;
use futures_retry::FutureRetry;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::time::Duration;

// TODO(zjn): make configurable. Short for local testing; long for real deployments
const RETRY_DELAY: Duration = Duration::from_millis(100);
const RETRY_ATTEMPTS: usize = 1000;

async fn get_start_time<C: Store>(config: &C) -> Result<DateTime<FixedOffset>, Error> {
    let key = vec!["experiment".to_string(), "start-time".to_string()];
    let start_time_str: String = config
        .get(key)
        .await?
        .ok_or_else(|| Error::new("Empty start time."))?;
    let start_time = DateTime::parse_from_rfc3339(&start_time_str)
        .map_err(|err| Error::new(&err.to_string()))?;
    Ok(start_time)
}

pub async fn set_start_time<C: Store>(config: &C, dt: DateTime<FixedOffset>) -> Result<(), Error> {
    let key = vec!["experiment".to_string(), "start-time".to_string()];
    config.put(key, dt.to_rfc3339()).await?;
    Ok(())
}

async fn wait_for_start_time_set_helper<C: Store>(
    config: &C,
    delay: Duration,
    attempts: usize,
) -> Result<DateTime<FixedOffset>, Error> {
    FutureRetry::new(
        move || get_start_time(config),
        error_policy(delay, attempts),
    )
    .await
}

pub async fn wait_for_start_time_set<C: Store>(config: &C) -> Result<DateTime<FixedOffset>, Error> {
    wait_for_start_time_set_helper(config, RETRY_DELAY, RETRY_ATTEMPTS).await
}

async fn has_quorum<C: Store>(config: &C, experiment: Experiment) -> Result<(), Error> {
    let nodes = resolve_all(config).await?;
    let actual: HashSet<Service> = HashSet::from_iter(nodes.iter().map(|node| node.service));
    let expected: HashSet<Service> = HashSet::from_iter(experiment.iter_services());

    if actual == expected {
        Ok(())
    } else {
        let msg = format!(
            "Bad quorum. \n\
             Expected {:?} but did not see.\n\"
             Got {:?} but did not expect to.",
            expected.difference(&actual),
            actual.difference(&expected)
        );
        Err(Error::new(&msg))
    }
}

async fn wait_for_quorum_helper<C: Store>(
    config: &C,
    experiment: Experiment,
    delay: Duration,
    attempts: usize,
) -> Result<(), Error> {
    FutureRetry::new(
        move || has_quorum(config, experiment),
        error_policy(delay, attempts),
    )
    .await
}

pub async fn wait_for_quorum<C: Store>(config: &C, experiment: Experiment) -> Result<(), Error> {
    wait_for_quorum_helper(config, experiment, RETRY_DELAY, RETRY_ATTEMPTS).await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::{factory::from_string, tests::inmem_stores},
        experiment::tests::experiments,
        net::tests::addrs,
        services::discovery::{register, tests::services, Node},
    };
    use futures::executor::block_on;
    use prop::bits::bool_vec;
    use proptest::prelude::*;
    use std::fmt::Debug;
    use std::iter::once;
    use std::net::SocketAddr;

    const NO_TIME: Duration = Duration::from_millis(0);

    fn addr() -> SocketAddr {
        SocketAddr::new("127.0.0.1".parse().unwrap(), 22)
    }

    prop_compose! {
        fn datetimes()
            (year in 1970i32..3000i32,
             month in 1u32..=12u32,
             day in 1u32..28u32,
             hours in 0u32..24u32,
             minutes in 0u32..60u32,
             seconds in 0u32..60u32) -> DateTime<FixedOffset> {
                FixedOffset::east(0)
                    .ymd(year, month, day)
                    .and_hms(hours, minutes, seconds)
            }
    }

    proptest! {
        #[test]
        fn test_set_and_get_start_time(config in inmem_stores(), dt in datetimes()) {
            futures::executor::block_on(async {
                set_start_time(&config, dt).await?;
                assert_eq!(get_start_time(&config).await?, dt);
                Ok::<(), Error>(())
            }).unwrap();
        }
    }

    #[tokio::test]
    async fn test_get_start_time_missing_entry() {
        let config = from_string("").unwrap();
        get_start_time(&config)
            .await
            .expect_err("Empty config should result in error.");
    }

    #[tokio::test]
    async fn test_get_start_time_malformed_entry() {
        let config = from_string("").unwrap();
        let key = vec!["experiment".to_string(), "start-time".to_string()];
        config
            .put(key, "not a valid RFC 3339 date".to_string())
            .await
            .unwrap();
        get_start_time(&config)
            .await
            .expect_err("Malformed entry should result in error.");
    }

    #[tokio::test]
    async fn test_wait_for_start_time_set_unset() {
        let config = from_string("").unwrap();
        wait_for_start_time_set_helper(&config, NO_TIME, 10)
            .await
            .expect_err("Should fail if start time is never set.");
    }

    #[tokio::test]
    async fn test_wait_for_start_time_set_okay() {
        let config = from_string("").unwrap();
        set_start_time(&config, DateTime::<FixedOffset>::from(Utc::now()))
            .await
            .unwrap();
        wait_for_start_time_set_helper(&config, NO_TIME, 10)
            .await
            .expect("Should succeed if start time is set.");
    }

    #[tokio::test]
    async fn test_wait_for_quorum_not_ready() {
        let config = from_string("").unwrap();
        let experiment = Experiment::new(1, 1);
        wait_for_quorum_helper(&config, experiment, NO_TIME, 10)
            .await
            .expect_err("Should fail if no quorum.");
    }

    #[tokio::test]
    async fn test_wait_for_quorum_okay() {
        let config = from_string("").unwrap();
        let experiment = Experiment::new(1, 1);
        for service in experiment.iter_services() {
            let node = Node::new(service, addr());
            register(&config, node).await.unwrap();
        }

        wait_for_quorum_helper(&config, experiment, NO_TIME, 10)
            .await
            .expect("Should succeed if quorum is ready.");
    }

    async fn run_quorum_test<C: Store, I: Iterator<Item = Node>>(
        config: &C,
        experiment: Experiment,
        nodes: I,
    ) -> Result<(), Error> {
        for node in nodes {
            register(config, node).await?;
        }
        has_quorum(config, experiment).await
    }

    fn experiments_and_nodes() -> impl Strategy<Value = (Experiment, Vec<Node>)> {
        experiments().prop_flat_map(|experiment| {
            let nodes = experiment
                .iter_services()
                .map(|service| Node::new(service, addr()))
                .collect();
            Just((experiment, nodes))
        })
    }

    // Given a vector, return a strategy for generating subsets of that vector.
    fn subset<T: Debug + Clone>(vec: Vec<T>) -> impl Strategy<Value = Vec<T>> {
        let count = vec.len();
        bool_vec::sampled(0..count, 0..count).prop_map(move |bits: Vec<bool>| {
            bits.iter()
                .zip(vec.iter().cloned())
                .filter_map(|(include, item)| match include {
                    true => Some(item),
                    false => None,
                })
                .collect()
        })
    }

    proptest! {
        #[test]
        fn test_has_quorum_too_many(
            config in inmem_stores(),
            (experiment, nodes) in experiments_and_nodes(),
            extra_service in services(),
            addr in addrs(),
        ) {
            let services: Vec<Service> =experiment.iter_services().collect();
            prop_assume!(!services.contains(&extra_service));
            let nodes = nodes.into_iter().chain(once(Node::new(extra_service, addr)));
            block_on(run_quorum_test(&config, experiment, nodes))
                .expect_err("Unexpected nodes--should error.");
        }

        #[test]
        fn test_has_quorum_too_few(
            config in inmem_stores(),
            (experiment, nodes) in experiments_and_nodes().prop_flat_map(|(experiment, nodes)|
                (Just(experiment), subset(nodes))
            ),
        ) {
            block_on(run_quorum_test(&config, experiment, nodes.into_iter()))
                .expect_err("Expected nodes missing--should error.");
        }

        #[test]
        fn test_has_quorum_just_right(
            config in inmem_stores(),
            (experiment, nodes) in experiments_and_nodes()
        ) {
            block_on(run_quorum_test(&config, experiment, nodes.into_iter()))
                .expect("Should have quorum.");
        }
    }
}
