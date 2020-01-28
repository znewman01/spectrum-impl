use crate::config;

use chrono::prelude::*;
use config::store::{Error, Store};
use std::time::Duration;
use tokio::time::delay_for;

const RETRY_DELAY: Duration = Duration::from_millis(1000);
const RETRY_ATTEMPTS: usize = 1000;

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
                // shouldn't need to sleep here but worker does stuff sync and weird
                // see e.g. https://github.com/hyperium/tonic/issues/252
                delay_for(Duration::from_millis(100)).await;
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

    #[tokio::test(threaded_scheduler)]
    async fn test_wait_for_quorum() {
        let store = config::factory::from_string("").expect("Failed to create store.");
        let handle = tokio::spawn(wait_for_quorum_helper(
            store.clone(),
            Duration::from_millis(0),
            2,
        ));
        tokio::task::yield_now().await;
        // TODO(zjn): verify that task isn't done yet
        let dt = DateTime::<FixedOffset>::from(Utc::now());
        set_quorum(&store, dt).await.unwrap();
        println!("yo done");
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
}
