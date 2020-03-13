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

// should be two parts:
// - clients by service: a Receiever<Option<Map>>
// this gets initialized on quorum and held by MyWorker
// - looking up WorkerInfo by ClientInfo
// this gets held by WorkerState and updaated on registerclient

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;

type PeersMap = HashMap<ClientInfo, Vec<WorkerInfo>>;

#[derive(Default)]
pub struct State {
    peers: PeersMap,
}

type ClientsMap = HashMap<WorkerInfo, SharedClient>;
type SharedLeaderClient = Arc<Mutex<LeaderClient<Channel>>>;

#[derive(Clone)]
struct InitState {
    clients: ClientsMap,
    leader: SharedLeaderClient,
}

impl InitState {
    async fn from_config<C: Store>(config: &C) -> Result<Self, Error> {
        let all_services = resolve_all(config).await?;

        let mut clients = ClientsMap::default();
        let peer_workers: Vec<_> = all_services
            .iter()
            .filter_map(|node| match node.service {
                Service::Worker(info) => Some((info, node.addr)),
                _ => None,
            })
            .collect();
        for (worker_info, addr) in peer_workers {
            let client = WorkerClient::connect(format!("http://{}", addr)).await?;
            clients.insert(worker_info, Arc::new(Mutex::new(client)));
        }

        let addr = all_services.iter().filter_map(|node| match node.service {
            Service::Leader(_) => Some(node.addr),
            _ => None,
        }).next().expect("Every node should have a corresponding leader.");
        let leader = Arc::new(Mutex::new(
            LeaderClient::connect(format!("http://{}", addr)).await?,
        ));

        Ok(InitState { clients, leader })
    }
}

pub struct ClientRegistry {
    state: RwLock<State>,
    init_state: watch::Receiver<Option<InitState>>,
}

pub struct Remote(watch::Sender<Option<InitState>>);

impl ClientRegistry {
    pub fn new_with_remote() -> (ClientRegistry, Remote) {
        let (tx, rx) = watch::channel(None);
        let registry = ClientRegistry {
            state: RwLock::default(),
            init_state: rx,
        };
        let remote = Remote(tx);
        (registry, remote)
    }

    fn get_peer(&self, info: WorkerInfo) -> Result<SharedClient, Status> {
        let lock = self.init_state.borrow();
        let peer_clients = &lock
            .as_ref()
            .expect("Should only insert after initialization.")
            .clients;
        let client: &SharedClient = peer_clients.get(&info).ok_or_else(|| {
            Status::failed_precondition(
                "All requested workers should be in the worker client registry",
            )
        })?;
        Ok(client.clone())
    }

    pub async fn get_peers(&self, client: &ClientInfo) -> Result<Vec<SharedClient>, Status> {
        let lock = self.state.read().await;
        lock.peers
            .get(client)
            .ok_or_else(|| {
                Status::failed_precondition(format!("Client info {:?} not registered.", client))
            })?
            .clone()
            .into_iter()
            .map(|worker| self.get_peer(worker))
            .collect::<Result<_, _>>()
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

    pub fn get_leader(&self) -> SharedLeaderClient {
        let lock = self.init_state.borrow();
        lock.as_ref()
            .expect("Should only get_leader() after initialization.")
            .leader
            .clone()
    }
}

impl Remote {
    pub async fn init<C>(&self, config: &C) -> Result<(), Error>
    where
        C: Store,
    {
        let init_state = InitState::from_config(config).await?;
        self.0
            .broadcast(Some(init_state))
            .or_else(|_| Err("Error sending client registry."))?;
        Ok(())
    }
}
