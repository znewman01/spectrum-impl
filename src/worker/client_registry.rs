use crate::proto::worker_client::WorkerClient;
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, ClientInfo, Service, WorkerInfo},
};

use log::{trace, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};
use tonic::{transport::Channel, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;

type PeersMap = HashMap<ClientInfo, Vec<WorkerInfo>>;
type ClientsMap = HashMap<WorkerInfo, SharedClient>;

async fn clients_map_from_config<C: Store>(config: &C) -> Result<ClientsMap, Error> {
    let mut map = ClientsMap::default();
    for (worker_info, addr) in find_peer_workers(config).await? {
        let client = WorkerClient::connect(format!("http://{}", addr)).await?;
        map.insert(worker_info, Arc::new(Mutex::new(client)));
    }
    Ok(map)
}

pub struct ClientRegistry {
    clients_peers: RwLock<PeersMap>,
    peers_workers: watch::Receiver<Option<ClientsMap>>,
}

pub struct ClientRegistryRemote(watch::Sender<Option<ClientsMap>>);

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
        let registry = ClientRegistry {
            clients_peers: RwLock::default(),
            peers_workers: rx,
        };
        let remote = ClientRegistryRemote(tx);
        (registry, remote)
    }

    fn get_peer(&self, info: WorkerInfo) -> Result<SharedClient, Status> {
        let lock = self.peers_workers.borrow();
        let map = &lock
            .as_ref()
            .expect("Should only insert after initialization.");
        let client: &SharedClient = map.get(&info).ok_or_else(|| {
            Status::failed_precondition(
                "All requested workers should be in the worker client registry",
            )
        })?;
        Ok(client.clone())
    }

    pub async fn get_peers(&self, client: &ClientInfo) -> Result<Vec<SharedClient>, Status> {
        let clients_peers = self.clients_peers.read().await;
        let clients = clients_peers
            .get(client)
            .ok_or_else(|| {
                Status::failed_precondition(format!("Client info {:?} not registered.", client))
            })?
            .clone()
            .into_iter()
            .map(|worker| self.get_peer(worker))
            .collect::<Result<_, _>>()?;
        Ok(clients)
    }

    pub async fn register_client(&self, client: &ClientInfo, shards: Vec<WorkerInfo>) {
        trace!("Registering client {:?}; shards: {:?}", &client, shards);
        let mut clients_peers = self.clients_peers.write().await;
        if clients_peers.insert(client.clone(), shards).is_some() {
            warn!("Client registered twice: {:?}", &client);
        }
    }

    pub async fn num_clients(&self) -> usize {
        // TODO(zjn): do something less heavy-weight then getting all the peers
        let lock = self.clients_peers.read().await;
        lock.len()
    }
}

impl ClientRegistryRemote {
    pub async fn init<C>(&self, config: &C) -> Result<(), Error>
    where
        C: Store,
    {
        let clients_map = clients_map_from_config(config).await?;
        self.0
            .broadcast(Some(clients_map))
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
        },
    };

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
