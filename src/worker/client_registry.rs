use crate::proto::{leader_client::LeaderClient, worker_client::WorkerClient};
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, ClientInfo, Service, WorkerInfo},
};

use log::{trace, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};
use tonic::{transport::Channel, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;
type WorkersMap = HashMap<WorkerInfo, SharedClient>;
type SharedLeaderClient = Arc<Mutex<LeaderClient<Channel>>>;

#[derive(Clone)]
struct ServiceMap {
    workers: WorkersMap,
    leader: SharedLeaderClient,
}

impl ServiceMap {
    async fn from_config<C: Store>(config: &C) -> Result<Self, Error> {
        let all_services = resolve_all(config).await?;

        let mut workers = WorkersMap::default();
        let peer_workers: Vec<_> = all_services
            .iter()
            .filter_map(|node| match node.service {
                Service::Worker(info) => Some((info, node.addr)),
                _ => None,
            })
            .collect();
        for (worker_info, addr) in peer_workers {
            let worker = WorkerClient::connect(format!("http://{}", addr)).await?;
            workers.insert(worker_info, Arc::new(Mutex::new(worker)));
        }

        let addr = all_services
            .iter()
            .filter_map(|node| match node.service {
                Service::Leader(_) => Some(node.addr),
                _ => None,
            })
            .next()
            .expect("Every node should have a corresponding leader.");
        let leader = Arc::new(Mutex::new(
            LeaderClient::connect(format!("http://{}", addr)).await?,
        ));

        Ok(ServiceMap { workers, leader })
    }
}

pub struct Remote(watch::Sender<Option<ServiceMap>>);

impl Remote {
    pub async fn init<C>(&self, config: &C) -> Result<(), Error>
    where
        C: Store,
    {
        let map = ServiceMap::from_config(config).await?;
        self.0
            .broadcast(Some(map))
            .or_else(|_| Err("Error sending service registry."))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ServiceRegistry(watch::Receiver<Option<ServiceMap>>);

impl ServiceRegistry {
    pub fn new_with_remote() -> (Self, Remote) {
        let (tx, rx) = watch::channel(None);
        let registry = ServiceRegistry(rx);
        let remote = Remote(tx);
        (registry, remote)
    }

    pub fn get_worker(&self, worker: WorkerInfo) -> Result<SharedClient, Status> {
        let lock = self.0.borrow();
        let workers = &lock
            .as_ref()
            .expect("Should only get_peer() after initialization.")
            .workers;
        let client: &SharedClient = workers.get(&worker).ok_or_else(|| {
            Status::failed_precondition(
                "All requested workers should be in the worker client registry",
            )
        })?;
        Ok(client.clone())
    }

    pub fn get_my_leader(&self) -> SharedLeaderClient {
        let lock = self.0.borrow();
        lock.as_ref()
            .expect("Should only get_leader() after initialization.")
            .leader
            .clone()
    }
}

type PeersMap = HashMap<ClientInfo, Vec<WorkerInfo>>;

#[derive(Default)]
pub struct State {
    peers: PeersMap,
}

#[derive(Default)]
pub struct ClientRegistry {
    state: RwLock<State>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        ClientRegistry {
            state: RwLock::default(),
        }
    }

    pub async fn get_peers(&self, client: &ClientInfo) -> Result<Vec<WorkerInfo>, Status> {
        let lock = self.state.read().await;
        lock.peers
            .get(client)
            .ok_or_else(|| {
                Status::failed_precondition(format!("Client info {:?} not registered.", client))
            })
            .map(|x| x.clone())
    }

    pub async fn register_client(&self, client: &ClientInfo, shards: Vec<WorkerInfo>) {
        trace!("Registering client {:?}; shards: {:?}", &client, shards);
        let mut lock = self.state.write().await;
        if lock.peers.insert(client.clone(), shards).is_some() {
            warn!("Client registered twice: {:?}", &client);
        }
    }

    pub async fn num_clients(&self) -> usize {
        // TODO(zjn): do something less heavy-weight then getting all the peers
        let lock = self.state.read().await;
        lock.peers.len()
    }
}
