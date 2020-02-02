#![allow(clippy::unit_arg)] // weird cargo clippy bug; complains about "derive(Arbitrary)"

use crate::config::store::{Error, Store};
use crate::services::discovery::{Group, LeaderInfo, PublisherInfo, Service, WorkerInfo};

use serde::{Deserialize, Serialize};
use std::iter::once;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct Experiment {
    // TODO(zjn): when nonzero types hit stable, replace u16 with NonZeroU16.
    // https://github.com/rust-lang/rfcs/blob/master/text/2307-concrete-nonzero-types.md
    groups: u16,
    workers_per_group: u16,
    pub clients: u16,
}

impl Experiment {
    pub fn new(groups: u16, workers_per_group: u16, clients: u16) -> Experiment {
        assert!(groups >= 1, "Expected at least 1 group.");
        assert!(
            workers_per_group >= 1,
            "Expected at least 1 worker per group."
        );
        assert!(clients >= 1, "Expected at least 1 client.");
        Experiment {
            groups,
            workers_per_group,
            clients,
        }
    }

    pub fn iter_services(self) -> impl Iterator<Item = Service> {
        let publishers = once((PublisherInfo {}).into());
        let groups = (0..self.groups).map(Group);
        let leaders = groups.clone().map(|group| (LeaderInfo { group }).into());
        let workers = groups.flat_map(move |group| {
            (0..self.workers_per_group).map(move |idx| (WorkerInfo { group, idx }).into())
        });

        publishers.chain(leaders).chain(workers)
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
    use core::ops::Range;
    use futures::executor::block_on;
    use proptest::prelude::*;

    pub fn experiments() -> impl Strategy<Value = Experiment> {
        let groups: Range<u16> = 1..10;
        let workers_per_group: Range<u16> = 1..10;
        let clients: Range<u16> = 1..10;
        (groups, workers_per_group, clients).prop_map(|(g, w, c)| Experiment::new(g, w, c))
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

        #[test]
        fn test_experiment_iter_services(experiment in experiments()) {
            let services: Vec<Service> = experiment.iter_services().collect();

            let mut publishers = vec![];
            let mut leaders = vec![];
            let mut workers = vec![];
            for service in services {
                match service {
                    Service::Publisher(_) => { publishers.push(service) },
                    Service::Leader(_) => { leaders.push(service) },
                    Service::Worker(_) => { workers.push(service) },
                }
            }
            let actual = (publishers.len(), leaders.len(), workers.len());
            let expected = (1,
                            experiment.groups as usize,
                            (experiment.groups * experiment.workers_per_group) as usize);
            prop_assert_eq!(actual, expected);
        }
    }
}
