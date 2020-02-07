use crate::proto::{worker_client::WorkerClient, ClientId, RegisterClientRequest, UploadRequest};
use crate::{
    config,
    services::{
        discovery::{resolve_all, Node},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, Group, Service,
    },
};
use config::store::Store;
use futures::prelude::*;
use log::{debug, info, trace};
use rand::{seq::IteratorRandom, thread_rng};
use std::collections::HashSet;
use std::iter::FromIterator;

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

pub async fn run<C, F>(
    config_store: C,
    _info: ClientInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Client starting");
    let start_time = wait_for_start_time_set(&config_store).await?;
    debug!("Received configuration from configuration server; initializing.");

    let shards: Vec<Node> = pick_worker_shards(resolve_all(&config_store).await?);
    let mut clients = vec![];
    for shard in shards {
        let mut client = WorkerClient::connect(format!("http://{}", shard.addr)).await?;
        let req = tonic::Request::new(RegisterClientRequest::default());
        client.register_client(req).await?;
        clients.push(client);
    }

    delay_until(start_time).await;

    let req = UploadRequest {
        client_id: Some(ClientId {
            client_id: "1".to_string(),
        }),
        share_and_proof: None,
    };

    for mut client in clients {
        let req = tonic::Request::new(req.clone());
        trace!("About to send upload request.");
        let response = client.upload(req).await?;
        debug!("RESPONSE={:?}", response.into_inner());
    }

    shutdown.await;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code)] // Compiler bug

    use super::*;
    use crate::experiment::{tests::experiments, Experiment};
    use proptest::prelude::*;
    use std::ops::Range;

    pub fn experiments_with_multiple_workers() -> impl Strategy<Value = Experiment> {
        let groups: Range<u16> = 1..10;
        let workers_per_group: Range<u16> = 2..10;
        let clients: Range<u16> = 1..10;
        (groups, workers_per_group, clients).prop_map(|(g, w, c)| Experiment::new(g, w, c))
    }

    proptest! {
        #[test]
        fn test_pick_worker_shards_subset(experiment in experiments()) {
            let services: HashSet<Service> = HashSet::from_iter(experiment.iter_services());
            let nodes: Vec<Node> = services.iter().cloned().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_services = HashSet::from_iter(shards.iter().map(|node| node.service));
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
