use crate::{
    config::store::{Error, Store},
    services::{discovery, retry::error_policy},
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

async fn has_quorum<C: Store>(config: &C) -> Result<(), Error> {
    // TODO(zjn): flesh out
    let workers: Vec<String> =
        discovery::resolve_all(config, discovery::ServiceType::Worker).await?;
    if workers.is_empty() {
        Err(Error::new("No workers yet."))
    } else {
        Ok(())
    }
}

async fn wait_for_quorum_helper<C: Store>(
    config: &C,
    delay: Duration,
    attempts: usize,
) -> Result<(), Error> {
    FutureRetry::new(|| has_quorum(config), error_policy(delay, attempts)).await
}

pub async fn wait_for_quorum<C: Store>(config: &C) -> Result<(), Error> {
    wait_for_quorum_helper(config, RETRY_DELAY, RETRY_ATTEMPTS).await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::{factory::from_string, tests::inmem_stores};
    use proptest::prelude::*;

    const NO_TIME: Duration = Duration::from_millis(0);

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
        wait_for_quorum_helper(&config, NO_TIME, 10)
            .await
            .expect_err("Should fail if no quorum.");
    }

    #[tokio::test]
    async fn test_wait_for_quorum_okay() {
        let config = from_string("").unwrap();
        discovery::register(&config, discovery::ServiceType::Worker, "1")
            .await
            .unwrap();
        wait_for_quorum_helper(&config, NO_TIME, 10)
            .await
            .expect("Should succeed if quorum is ready.");
    }

    #[tokio::test]
    async fn test_has_quorum_no_quorum() {
        let config = from_string("").unwrap();
        has_quorum(&config)
            .await
            .expect_err("Should not have quorum.");
    }

    #[tokio::test]
    async fn test_has_quorum() {
        let config = from_string("").unwrap();
        discovery::register(&config, discovery::ServiceType::Worker, "1")
            .await
            .unwrap();
        has_quorum(&config).await.expect("Should have quorum.");
    }
}
