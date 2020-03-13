use crate::proto::worker_client::WorkerClient;
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, Service, WorkerInfo},
};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tonic::{transport::Channel, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;

type Map = HashMap<WorkerInfo, SharedClient>;

#[derive(Default, Clone)]
pub struct State {
    map: Map,
}

impl State {
    pub async fn from_config<C: Store>(config: &C) -> Result<State, Error> {
        let mut state = State::default();
        for (worker_info, addr) in find_peer_workers(config).await? {
            let client = WorkerClient::connect(format!("http://{}", addr)).await?;
            state.map.insert(worker_info, Arc::new(Mutex::new(client)));
        }
        Ok(state)
    }
}

#[derive(Clone)]
pub struct ClientRegistry(watch::Receiver<Option<State>>);

pub struct ClientRegistryRemote(watch::Sender<Option<State>>);

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
    pub fn new_with_remote() -> (ClientRegistry, ClientRegistryRemote) {
        let (tx, rx) = watch::channel(None);
        let registry = ClientRegistry(rx);
        let remote = ClientRegistryRemote(tx);
        (registry, remote)
    }

    pub fn get(&self, info: WorkerInfo) -> Result<SharedClient, Status> {
        let lock = self.0.borrow();
        let map = &lock
            .as_ref()
            .expect("Should only insert after initialization.")
            .map;
        let client: &SharedClient = map.get(&info).ok_or_else(|| {
            Status::failed_precondition(
                "All requested workers should be in the worker client registry",
            )
        })?;
        Ok(client.clone())
    }
}

impl ClientRegistryRemote {
    pub async fn init<C>(&self, config: &C) -> Result<(), Error>
    where
        C: Store,
    {
        let state = State::from_config(config).await?;
        self.0
            .broadcast(Some(state))
            .or_else(|_| Err("Error sending client registry."))?;
        Ok(())
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
