#![allow(clippy::unit_arg)] // weird cargo clippy bug; complains about "derive(Arbitrary)"

use crate::config::store::{Error, Store};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Experiment {
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    pub groups: u16,
    pub workers_per_group: u16,
}

impl Experiment {
    pub fn new(groups: u16, workers_per_group: u16) -> Experiment {
        assert!(groups >= 1, "Expected at least 1 group.");
        assert!(
            workers_per_group >= 1,
            "Expected at least 1 worker per group."
        );
        Experiment {
            groups,
            workers_per_group,
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
pub mod tests {
    use super::*;
    use crate::config::tests::inmem_stores;
    use futures::executor::block_on;
    use proptest::prelude::*;

    pub fn experiments() -> impl Strategy<Value = Experiment> {
        let groups: core::ops::Range<u16> = 1..10;
        let workers_per_group: core::ops::Range<u16> = 1..10;
        (groups, workers_per_group).prop_map(|(g, w)| Experiment::new(g, w))
    }

    proptest! {
        #[test]
        fn test_experiment_roundtrip(config in inmem_stores(), experiment in experiments()) {
            block_on(async {
                write_to_store(&config, experiment).await.unwrap();
                assert_eq!(
                    read_from_store(&config).await.unwrap(),
                    experiment);
            });
        }
    }
}
