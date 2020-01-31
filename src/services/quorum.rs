use crate::{
    config::store::{Error, Store},
    experiment::Experiment,
    services::{
        discovery,
        discovery::Service::{Leader, Publisher, Worker},
        retry::error_policy,
    },
};

use chrono::prelude::*;
use futures_retry::FutureRetry;
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
    FutureRetry::new(|| get_start_time(config), error_policy(delay, attempts)).await
}

pub async fn wait_for_start_time_set<C: Store>(config: &C) -> Result<DateTime<FixedOffset>, Error> {
    wait_for_start_time_set_helper(config, RETRY_DELAY, RETRY_ATTEMPTS).await
}

async fn has_quorum<C: Store>(config: &C, experiment: Experiment) -> Result<(), Error> {
    let nodes = discovery::resolve_all(config).await?;
    let leaders = nodes
        .iter()
        .filter(|node| match node.service {
            Leader { .. } => true,
            _ => false,
        })
        .count();
    let workers = nodes
        .iter()
        .filter(|node| match node.service {
            Worker { .. } => true,
            _ => false,
        })
        .count();
    let publishers = nodes
        .iter()
        .filter(|node| match node.service {
            Publisher => true,
            _ => false,
        })
        .count();
    let actual = (leaders, workers, publishers);

    let expected_leaders = experiment.groups as usize;
    let expected_workers = (experiment.groups * experiment.workers_per_group) as usize;
    let expected_publishers = 1 as usize;
    let expected = (expected_leaders, expected_workers, expected_publishers);

    if actual == expected {
        Ok(())
    } else {
        let msg = format!(
            "Bad quorum count. Expected {:?}, got {:?} (leaders, workers, publishers).",
            expected, actual
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
        || has_quorum(config, experiment),
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
    use crate::config::{factory::from_string, tests::inmem_stores};
    use crate::experiment::tests::experiments;
    use discovery::{
        register, Group, Node,
        Service::{Leader, Publisher, Worker},
    };
    use futures::executor::block_on;
    use proptest::prelude::*;
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
        register(
            &config,
            Node::new(
                Worker {
                    group: Group(0),
                    idx: 0,
                },
                addr(),
            ),
        )
        .await
        .unwrap();
        register(&config, Node::new(Leader { group: Group(0) }, addr()))
            .await
            .unwrap();
        register(&config, Node::new(Publisher, addr()))
            .await
            .unwrap();

        wait_for_quorum_helper(&config, experiment, NO_TIME, 10)
            .await
            .expect("Should succeed if quorum is ready.");
    }

    async fn run_quorum_test<C: Store>(
        config: &C,
        experiment: Experiment,
        leaders: u16,
        workers: u16,
        publishers: u16,
    ) -> Result<(), Error> {
        for leader_idx in 0..leaders {
            let leader = Leader {
                group: Group(leader_idx),
            };
            register(config, Node::new(leader, addr())).await?;
        }
        for worker_idx in 0..workers {
            let worker = Worker {
                group: Group(0),
                idx: worker_idx,
            };
            register(config, Node::new(worker, addr())).await?;
        }
        for _ in 0..publishers {
            let publisher = Publisher;
            register(config, Node::new(publisher, addr())).await?;
        }

        has_quorum(config, experiment).await
    }

    proptest! {
        #[test]
        fn test_has_quorum_no_publisher(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups ;
            let workers = experiment.workers_per_group * experiment.groups;
            let publishers = 0;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect_err("No publisher--should error.");
        }

        #[test]
        fn test_has_quorum_too_few_leaders(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups - 1;
            let workers = experiment.workers_per_group * experiment.groups;
            let publishers = 1;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect_err("Not enough leaders--should error.");
        }

        #[test]
        fn test_has_quorum_too_few_workers(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups;
            let workers = experiment.workers_per_group * experiment.groups - 1;
            let publishers = 1;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect_err("Not enough workers--should error.");
        }

        #[test]
        fn test_has_quorum_just_right(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups;
            let workers = experiment.workers_per_group * experiment.groups;
            let publishers = 1;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect("Should have quorum.");
        }

        #[test]
        fn test_has_quorum_too_many_leaders(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups + 1;
            let workers = experiment.workers_per_group * experiment.groups;
            let publishers = 1;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect_err("Too many leaders--should error.");
        }

        #[test]
        fn test_has_quorum_too_many_workers(
            config in inmem_stores(),
            experiment in experiments()
        ) {
            let leaders = experiment.groups;
            let workers = experiment.workers_per_group * experiment.groups + 1;
            let publishers = 1;
            block_on(run_quorum_test(&config, experiment, leaders, workers, publishers))
                .expect_err("Too many workers--should error.");
        }

    }
}
