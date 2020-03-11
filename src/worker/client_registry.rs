use crate::proto::worker_client::WorkerClient;
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, Service, WorkerInfo},
};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Channel, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;

#[derive(Default, Clone)]
pub struct ClientRegistry(HashMap<WorkerInfo, SharedClient>);

async fn find_peer_workers<C: Store>(config: &C) -> Result<Vec<(WorkerInfo, SocketAddr)>, Error> {
    Ok(resolve_all(config)
        .await?
        .iter()
        .filter_map(|node| match node.service {
            Service::Worker(info) => Some((info, node.addr)),
            _ => None,
        })
        .collect())
}

impl ClientRegistry {
    pub fn insert(&mut self, info: WorkerInfo, client: WorkerClient<Channel>) {
        self.0.insert(info, Arc::new(Mutex::new(client)));
    }

    pub fn get(&self, info: WorkerInfo) -> Result<SharedClient, Status> {
        let client: &SharedClient = self.0.get(&info).ok_or_else(|| {
            Status::failed_precondition(
                "All requested workers should be in the worker client registry",
            )
        })?;
        Ok(client.clone())
    }

    pub async fn from_config<C: Store>(config: &C) -> Result<ClientRegistry, Error> {
        let mut registry = ClientRegistry::default();
        for (worker_info, addr) in find_peer_workers(config).await? {
            let client = WorkerClient::connect(format!("http://{}", addr)).await?;
            registry.insert(worker_info, client);
        }
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config,
        experiment::Experiment,
        services::{
            discovery::{register, Node},
            Group,
        },
    };
    use std::ops::Deref;

    #[should_panic]
    #[test]
    fn test_client_registry_get_missing() {
        let reg = ClientRegistry::default();
        reg.get(WorkerInfo::new(Group::new(0), 0)).unwrap();
    }

    #[tokio::test]
    async fn test_client_registry_get_and_insert() {
        let mut reg = ClientRegistry::default();
        let info = WorkerInfo::new(Group::new(0), 0);
        let client = WorkerClient::connect("http://www.example.com:80")
            .await
            .unwrap();

        reg.insert(info, client);
        let client_mutex = reg.get(info).unwrap();
        let client_lock = client_mutex.lock().await;
        let _: &WorkerClient<Channel> = client_lock.deref();
        // if we got /any/ client back, it's okay
    }

    #[tokio::test]
    async fn test_find_peer_workers_empty() {
        let config = config::factory::from_string("").unwrap();

        let peers = find_peer_workers(&config).await.unwrap();

        assert_eq!(peers.len(), 0);
    }

    #[tokio::test]
    async fn test_find_peer_workers() {
        let config = config::factory::from_string("").unwrap();

        let experiment = Experiment::new(2, 2, 2, 0);
        for service in experiment.iter_services() {
            let node = Node::new(service, "127.0.0.1:22".parse().unwrap());
            register(&config, node).await.unwrap()
        }

        let peers = find_peer_workers(&config).await.unwrap();
        assert_eq!(peers.len(), 4);
    }
}
