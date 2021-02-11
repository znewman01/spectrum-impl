use crate::proto::{worker_client::WorkerClient, RegisterClientRequest};
use crate::Error;
use crate::{
    config,
    services::{
        discovery::{resolve_all, Node},
        ClientInfo, Group, Service,
    },
};
use config::store::Store;

use log::{debug, trace};
use rand::{seq::IteratorRandom, thread_rng};
use tokio::time::sleep;
use tonic::transport::{channel::Channel, Certificate, ClientTlsConfig, Uri};

use std::collections::HashSet;
use std::time::Duration;

type TokioError = Box<dyn std::error::Error + Sync + Send>;

// Picks one worker from each group.
fn pick_worker_shards(nodes: Vec<Node>) -> Vec<Node> {
    let workers: Vec<Node> = nodes
        .into_iter()
        .filter(|node| matches!(node.service, Service::Worker(_)))
        .collect();
    let groups: HashSet<Group> = workers
        .iter()
        .map(|node| match node.service {
            Service::Worker(info) => info.group,
            _ => panic!("Already filtered to just workers."),
        })
        .collect();
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

async fn connect(
    addr: String,
    cert: Option<Certificate>,
) -> Result<WorkerClient<Channel>, TokioError> {
    let mut attempts: u8 = 0;
    let uri = format!("http://{}", addr)
        .parse::<Uri>()
        .expect("Invalid URI");
    loop {
        let mut builder = Channel::builder(uri.clone());
        if let Some(ref cert) = cert {
            debug!("TLS for client.");
            builder = builder.tls_config(
                ClientTlsConfig::new()
                    .domain_name("spectrum.example.com")
                    .ca_certificate(cert.clone()),
            )?;
        }
        let res = builder.connect().await;
        if let Ok(channel) = res {
            return Ok(WorkerClient::new(channel));
        }
        debug!("Failed to connect to worker: {:?}", res);
        attempts += 1;
        if attempts >= 10 {
            return Err(Box::new(Error::from(format!(
                "Failed to connect to worker on every attempt! {:?}",
                res
            ))));
        }
        sleep(Duration::from_millis(50)).await;
    }
}

pub async fn connect_and_register<C>(
    config: &C,
    info: ClientInfo,
    cert: Option<Certificate>,
) -> Result<Vec<WorkerClient<Channel>>, TokioError>
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
        let mut client = connect(shard.addr.clone(), cert.clone()).await?;
        let req = tonic::Request::new(req.clone());
        trace!("Registering with shard {}...", shard.addr);
        client.register_client(req).await?;
        trace!("Registered with shard {}!", shard.addr);
        clients.push(client);
    }
    Ok(clients)
}

#[cfg(test)]
mod tests {
    #![allow(unreachable_code)] // Compiler bug

    use super::*;
    use crate::experiment::Experiment;
    use proptest::prelude::*;

    pub fn experiments_with_multiple_workers() -> impl Strategy<Value = Experiment> {
        any::<Experiment>().prop_filter(
            "Only want experiments with multiple workers per group",
            |e| e.group_size() > 1,
        )
    }

    proptest! {
        #[test]
        fn test_pick_worker_shards_subset(experiment: Experiment) {
            let services: HashSet<Service> = experiment.iter_services().collect();
            let nodes: Vec<Node> = services.iter().cloned().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_services: HashSet<_> = shards.iter().map(|node| node.service.clone()).collect();
            prop_assert!(shard_services.is_subset(&services));
        }

        #[test]
        fn test_pick_worker_shards_distinct_groups(experiment: Experiment) {
            let nodes: Vec<Node> = experiment.iter_services().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_groups: HashSet<Group> = shards
                .iter()
                .cloned()
                .map(|node| match node.service {
                    Service::Worker(info) => info.group,
                    _ => { panic!("All shards should be workers."); }
                })
                .collect();
            prop_assert_eq!(shard_groups.len(), shards.len(),
                            "Each shard should be from a distinct group.");
        }

        #[test]
        fn test_pick_worker_shards_all_groups(experiment: Experiment) {
            let services: Vec<Service> = experiment.iter_services().collect();
            let nodes: Vec<Node> = services.iter().cloned().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards = pick_worker_shards(nodes);

            let shard_groups: HashSet<Group> = shards.iter()
                    .cloned()
                    .map(|node| match node.service {
                        Service::Worker(info) => info.group,
                        _ => { panic!("All shards should be workers."); }
                    }).collect();
            let original_groups: HashSet<Group> = services
                .iter()
                .filter_map(|service| match service {
                    Service::Worker(info) => Some(info.group),
                    _ => None
                }).collect() ;

            prop_assert_eq!(shard_groups.len(), original_groups.len(),
                            "Each group should be represented.");
        }

        #[test]
        fn test_pick_worker_shards_different(experiment in experiments_with_multiple_workers()) {
            let nodes: Vec<Node> = experiment.iter_services().map(|service| {
                Node::new(service, "127.0.0.1:22".parse().unwrap())
            }).collect();

            let shards: HashSet<Node> = pick_worker_shards(nodes.clone()).into_iter().collect();
            // Worst-case is 1 group, 2 workers per group. In this setting, the
            // odds that we pick the same thing every time are 1/2^{t-1} (where
            // t is the number of trials). We can live with a 1 in 2^20 failure
            // rate.
            for _ in 0..20 {
                let shards_prime: HashSet<_> = pick_worker_shards(nodes.clone()).into_iter().collect();
                if shards != shards_prime {
                    return Ok(());
                }
            }
            panic!("Got through all trials without picking different shards.");
        }
    }
}
