use crate::services::{ClientInfo, WorkerInfo};

use log::{trace, warn};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tonic::Status;

type PeersMap = HashMap<ClientInfo, Vec<WorkerInfo>>;

#[derive(Default)]
pub struct State {
    peers: PeersMap,
}

#[derive(Default)]
pub struct Registry {
    state: RwLock<State>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
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
