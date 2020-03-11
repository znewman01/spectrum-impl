use crate::proto::{worker_client::WorkerClient, RegisterClientRequest};
use crate::{
    config,
    services::{
        discovery::{resolve_all, Node},
        ClientInfo, Group, Service,
    },
};
use config::store::Store;
use rand::{seq::IteratorRandom, thread_rng};
use std::collections::HashSet;
use std::iter::FromIterator;

type TokioError = Box<dyn std::error::Error + Sync + Send>;

// Picks one worker from each group.
fn pick_worker_shards(nodes: Vec<Node>) -> Vec<Node> {
    let workers: Vec<Node> = nodes
        .into_iter()
        .filter(|node| {
            if let Service::Worker(_) = node.service {
                true
            } else {
                false
            }
        })
        .collect();
    let groups: HashSet<Group> =
        HashSet::from_iter(workers.iter().map(|node| match node.service {
            Service::Worker(info) => info.group,
            _ => panic!("Already filtered to just workers."),
        }));
    let mut shards = vec![];
    for group in groups {
        let workers_in_group = workers.iter().filter(|node| match node.service {
            Service::Worker(info) => info.group == group,
            _ => panic!("Already filtered to just workers."),
        });
        shards.push(
            workers_in_group
                .choose(&mut thread_rng())
                .expect("workers_in_group must be non-empty.")
                .clone(),
        );
    }
    shards
}

pub async fn connect_and_register<C>(
    config: &C,
    info: ClientInfo,
) -> Result<Vec<WorkerClient<tonic::transport::channel::Channel>>, TokioError>
where
    C: Store,
{
    let nodes: Vec<Node> = resolve_all(config).await?;
    let shards: Vec<Node> = pick_worker_shards(nodes);
    let mut clients = vec![];
    let req = RegisterClientRequest {
        client_id: Some(info.to_proto()),
        shards: shards
            .iter()
            .map(|shard| match shard.service {
                Service::Worker(worker_info) => worker_info.into(),
                _ => panic!("Non-worker node."),
            })
            .collect(),
    };
    for shard in shards {
        let mut client = WorkerClient::connect(format!("http://{}", shard.addr)).await?;
        let req = tonic::Request::new(req.clone());
        client.register_client(req).await?;
        clients.push(client);
    }
    Ok(clients)
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code)] // Compiler bug

    use super::*;
    use crate::experiment::{tests::experiments, Experiment};
    use proptest::prelude::*;

    pub fn experiments_with_multiple_workers() -> impl Strategy<Value = Experiment> {
        experiments().prop_filter("Only want experiments with multiple workers", |e| {
            e.workers_per_group > 1
        })
    }

    proptest! {
        #[test]
        fn test_pick_worker_shards_subset(experiment in experiments()) {
            let services: HashSet<Service> = HashSet::from_iter(experiment.iter_services());
            let nodes: Vec<Node> = services.iter().cloned().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_services = HashSet::from_iter(shards.iter().map(|node| node.service.clone()));
            prop_assert!(shard_services.is_subset(&services));
        }

        #[test]
        fn test_pick_worker_shards_distinct_groups(experiment in experiments()) {
            let nodes: Vec<Node> = experiment.iter_services().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_groups: HashSet<Group> = HashSet::from_iter(
                shards.iter()
                    .cloned()
                    .map(|node| match node.service {
                        Service::Worker(info) => info.group,
                        _ => { panic!("All shards should be workers."); }
                    })
            );
            prop_assert_eq!(shard_groups.len(), shards.len(),
                            "Each shard should be from a distinct group.");
        }

        #[test]
        fn test_pick_worker_shards_all_groups(experiment in experiments()) {
            let services: Vec<Service> = experiment.iter_services().collect();
            let nodes: Vec<Node> = services.iter().cloned().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_groups: HashSet<Group> = HashSet::from_iter(
                shards.iter()
                    .cloned()
                    .map(|node| match node.service {
                        Service::Worker(info) => info.group,
                        _ => { panic!("All shards should be workers."); }
                    })
            );
            let original_groups: HashSet<Group> = HashSet::from_iter(
                services.iter()
                    .filter_map(|service| match service {
                        Service::Worker(info) => Some(info.group),
                        _ => None
                    })
            );

            prop_assert_eq!(shard_groups.len(), original_groups.len(),
                            "Each group should be represented.");
        }

        #[test]
        fn test_pick_worker_shards_different(experiment in experiments_with_multiple_workers()) {
            let nodes: Vec<Node> = experiment.iter_services().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards: HashSet<Node> = HashSet::from_iter(pick_worker_shards(nodes.clone()).into_iter());
            // Worst-case is 1 group, 2 workers per group. In this setting, the
            // odds that we pick the same thing every time are 1/2^{t-1} (where
            // t is the number of trials). We can live with a 1 in 2^20 failure
            // rate.
            for _ in 0..20 {
                let shards_prime = HashSet::from_iter(pick_worker_shards(nodes.clone()).into_iter());
                if shards != shards_prime {
                    return Ok(());
                }
            }
            panic!("Got through all trials without picking different shards.");
        }
    }
}
