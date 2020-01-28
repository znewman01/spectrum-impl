use crate::config;

use config::store::{Error, Store};
use std::time::Duration;
use tokio::time::delay_for;

const RETRY_DELAY: Duration = Duration::from_millis(1000);
const RETRY_ATTEMPTS: usize = 1000;

async fn has_quorum<C: Store>(config: &C) -> Result<bool, Error> {
    let key = vec!["workers".to_string()];
    let value = config.list(key).await?;
    Ok(!value.is_empty())
}

pub async fn set_quorum<C: Store>(config: &C) -> Result<(), Error> {
    let key = vec!["workers".to_string(), "1".to_string()];
    config.put(key, "ok".to_string()).await?;
    Ok(())
}

async fn wait_for_quorum_helper<C: Store>(
    config: C,
    delay: Duration,
    attempts: usize,
) -> Result<(), Error> {
    for _ in 0..attempts {
        println!("checking");
        if has_quorum(&config).await? {
            // shouldn't need to sleep here but worker does stuff sync and weird
            // see e.g. https://github.com/hyperium/tonic/issues/252
            delay_for(Duration::from_millis(100)).await;
            return Ok(());
        }
        delay_for(delay).await;
    }
    let msg = format!("Quorum not reached after {} attempts", attempts);
    Err(Error::new(&msg))
}

pub async fn wait_for_quorum<C: Store>(config: C) -> Result<(), Error> {
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
        set_quorum(&store).await.unwrap();
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
