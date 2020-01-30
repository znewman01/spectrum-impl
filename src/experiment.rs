#![allow(clippy::unit_arg)] // weird cargo clippy bug; complains about "derive(Arbitrary)"

use crate::config::store::{Error, Store};

#[cfg(test)] // TODO(zjn): move to mod tests (https://github.com/AltSysrq/proptest/pull/106)
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Experiment {
    num_groups: u16,
    num_workers_per_group: u16,
}

impl Experiment {
    #[allow(dead_code)]
    pub fn new() -> Experiment {
        Experiment {
            num_groups: 0,
            num_workers_per_group: 0,
        }
    }
}

pub async fn write_to_store<C: Store>(config: &C, experiment: Experiment) -> Result<(), Error> {
    let json_str =
        serde_json::to_string(&experiment).map_err(|err| Error::new(&err.to_string()))?;
    config
        .put(
            vec!["experiment".to_string(), "config".to_string()],
            json_str,
        )
        .await?;
    Ok(())
}

pub async fn read_from_store<C: Store>(config: &C) -> Result<Experiment, Error> {
    let json_str = config
        .get(vec!["experiment".to_string(), "config".to_string()])
        .await?
        .ok_or_else(|| Error::new("No experiment string in store."))?;
    Ok(serde_json::from_str(&json_str).map_err(|err| Error::new(&err.to_string()))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::inmem_stores;
    use futures::executor::block_on;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_experiment_roundtrip(config in inmem_stores(), experiment in any::<Experiment>()) {
            block_on(async {
                write_to_store(&config, experiment).await.unwrap();
                assert_eq!(
                    read_from_store(&config).await.unwrap(),
                    experiment);
            });
        }
    }
}
