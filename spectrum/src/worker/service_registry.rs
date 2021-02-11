use crate::proto::{leader_client::LeaderClient, worker_client::WorkerClient};
use crate::{
    config::store::Store,
    services::{discovery::resolve_all, Service, WorkerInfo},
};

use log::debug;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tonic::{
    transport::Certificate, transport::Channel, transport::ClientTlsConfig, transport::Uri, Status,
};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;
type WorkersMap = HashMap<WorkerInfo, SharedClient>;
type SharedLeaderClient = Arc<Mutex<LeaderClient<Channel>>>;

#[derive(Clone)]
struct Map {
    workers: WorkersMap,
    leader: Option<SharedLeaderClient>,
}

impl Map {
    async fn from_config<C: Store>(
        worker: WorkerInfo,
        config: &C,
        tls: Option<Certificate>,
    ) -> Result<Self, Error> {
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
            let uri = format!("https://{}", addr)
                .parse::<Uri>()
                .expect("bad addr");
            let mut builder = Channel::builder(uri);
            if let Some(ref cert) = tls {
                debug!("TLS for WorkerClient.");
                builder = builder
                    .tls_config(
                        ClientTlsConfig::new()
                            .domain_name("spectrum.example.com")
                            .ca_certificate(cert.clone()),
                    )
                    .map_err(|e| format!("{:?}", e))?;
            }
            let channel = builder.connect().await.map_err(|err| err.to_string())?;
            let worker = WorkerClient::new(channel);
            workers.insert(worker_info, Arc::new(Mutex::new(worker)));
        }

        let addr = all_services
            .into_iter()
            .find_map(|node| match node.service {
                Service::Leader(leader) if leader.group == worker.group => Some(node.addr),
                _ => None,
            });
        let leader = if let Some(addr) = addr {
            Some(Arc::new(Mutex::new(
                LeaderClient::connect(format!("http://{}", addr)).await?,
            )))
        } else {
            None
        };

        Ok(Map { workers, leader })
    }
}

pub struct Remote(watch::Sender<Option<Map>>);

impl Remote {
    pub async fn init<C>(
        &self,
        worker: WorkerInfo,
        config: &C,
        tls: Option<Certificate>,
    ) -> Result<(), Error>
    where
        C: Store,
    {
        let map = Map::from_config(worker, config, tls).await?;
        self.0
            .send(Some(map))
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
            .as_ref()
            .expect("Don't call get_my_leader() in hammer mode.")
            .clone()
    }
}
