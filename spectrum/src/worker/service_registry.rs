use crate::proto::{leader_client::LeaderClient, worker_client::WorkerClient};
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, Service, WorkerInfo},
};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tonic::{transport::Channel, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;
type WorkersMap = HashMap<WorkerInfo, SharedClient>;
type SharedLeaderClient = Arc<Mutex<LeaderClient<Channel>>>;

#[derive(Clone)]
struct Map {
    workers: WorkersMap,
    leader: SharedLeaderClient,
}

impl Map {
    async fn from_config<C: Store>(worker: WorkerInfo, config: &C) -> Result<Self, Error> {
        let all_services = resolve_all(config).await?;

        let mut workers = WorkersMap::default();
        let peer_workers: Vec<_> = all_services
            .iter()
            .filter_map(|node| match node.service {
                Service::Worker(info) => Some((info, node.addr.clone())),
                _ => None,
            })
            .collect();
        for (worker_info, addr) in peer_workers {
            let worker = WorkerClient::connect(format!("http://{}", addr)).await?;
            workers.insert(worker_info, Arc::new(Mutex::new(worker)));
        }

        let addr = all_services
            .into_iter()
            .find_map(|node| match node.service {
                Service::Leader(leader) if leader.group == worker.group => Some(node.addr),
                _ => None,
            })
            .expect("Every node should have a corresponding leader.");
        let leader = Arc::new(Mutex::new(
            LeaderClient::connect(format!("http://{}", addr)).await?,
        ));

        Ok(Map { workers, leader })
    }
}

pub struct Remote(watch::Sender<Option<Map>>);

impl Remote {
    pub async fn init<C>(&self, worker: WorkerInfo, config: &C) -> Result<(), Error>
    where
        C: Store,
    {
        let map = Map::from_config(worker, config).await?;
        self.0
            .broadcast(Some(map))
            .map_err(|_| "Error sending service registry.")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Registry(watch::Receiver<Option<Map>>);

impl Registry {
    pub fn new_with_remote() -> (Self, Remote) {
        let (tx, rx) = watch::channel(None);
        let registry = Registry(rx);
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
