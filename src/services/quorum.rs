use crate::config;

use chrono::prelude::*;
use config::store::{Error, Store};
use std::time::Duration;
use tokio::time::delay_for;

// TODO(zjn): make configurable. Short for local testing; long for real deployments
const RETRY_DELAY: Duration = Duration::from_millis(100);
const RETRY_ATTEMPTS: usize = 1000;

async fn get_start_time<C: Store>(config: &C) -> Result<Option<DateTime<FixedOffset>>, Error> {
    let key = vec!["experiment".to_string(), "start-time".to_string()];
    let start_time_str: Option<String> = config.get(key).await?;
    start_time_str
        .map(|s| DateTime::parse_from_rfc3339(&s).map_err(|err| Error::new(&err.to_string())))
        .transpose()
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
    for _ in 0..attempts {
        match get_start_time(config).await? {
            Some(start_time) => {
                return Ok(start_time);
            }
            None => {
                delay_for(delay).await;
            }
        };
    }
    let msg = format!("Start time not observed set after {} attempts", attempts);
    Err(Error::new(&msg))
}

pub async fn wait_for_start_time_set<C: Store>(config: &C) -> Result<DateTime<FixedOffset>, Error> {
    wait_for_start_time_set_helper(config, RETRY_DELAY, RETRY_ATTEMPTS).await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config;
    use futures::FutureExt;
    use tokio::sync::oneshot::{channel, error::TryRecvError};
    use tokio::task::yield_now;

    #[tokio::test(threaded_scheduler)]
    async fn test_wait_for_start_time_set() {
        let store = config::factory::from_string("").expect("Failed to create store.");
        let inner_store = store.clone();
        let (tx, mut rx) = channel();
        let handle = tokio::spawn(async move {
            wait_for_start_time_set_helper(&inner_store, Duration::from_millis(0), 2)
                .inspect(|_| tx.send(()).unwrap())
                .await
        });
        yield_now().await;

        assert_eq!(
            rx.try_recv().expect_err("Task not have completed yet."),
            TryRecvError::Empty
        );

        let dt = DateTime::<FixedOffset>::from(Utc::now());
        set_start_time(&store, dt).await.unwrap();

        let result = handle
            .await
            .expect("Task should have completed without crashing.");
        result.expect("Task should have been successsful.");
    }

    #[tokio::test]
    async fn test_wait_for_start_time_set_failure() {
        let store = config::factory::from_string("").expect("Failed to create store.");
        let result = wait_for_start_time_set_helper(&store, Duration::from_millis(0), 1).await;
        result.expect_err("Expected failure, as quorum never reached.");
    }
}
