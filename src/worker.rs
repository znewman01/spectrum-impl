use crate::proto::{
    worker_client::WorkerClient,
    worker_server::{Worker, WorkerServer},
    RegisterClientRequest, RegisterClientResponse, ShareCheck, ShareWithProof, UploadRequest,
    UploadResponse, VerifyRequest, VerifyResponse,
};
use crate::{
    config::store::Store,
    net::get_addr,
    services::{
        discovery::{register, resolve_all, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, Service, WorkerInfo,
    },
};

use futures::prelude::*;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::iter::FromIterator;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{
    spawn,
    sync::{watch, Mutex, RwLock},
    task::spawn_blocking,
};
use tonic::{transport::Channel, Request, Response, Status};

type Error = Box<dyn std::error::Error + Sync + Send>;
type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;

#[derive(Default, Clone)]
struct ClientRegistry(HashMap<WorkerInfo, SharedClient>);

async fn find_peer_workers<C: Store>(
    config: &C,
    worker: WorkerInfo,
) -> Result<Vec<(WorkerInfo, SocketAddr)>, Error> {
    Ok(resolve_all(config)
        .await?
        .iter()
        .filter_map(|node| match node.service {
            Service::Worker(info) => Some(info)
                .filter(|&info| info != worker)
                .map(|info| (info, node.addr)),
            _ => None,
        })
        .collect())
}

impl ClientRegistry {
    fn insert(&mut self, info: WorkerInfo, client: WorkerClient<Channel>) {
        self.0.insert(info, Arc::new(Mutex::new(client)));
    }

    // TODO(zjn): return Result<SharedClient, Status>
    fn get(&self, info: WorkerInfo) -> SharedClient {
        let client: &SharedClient = self
            .0
            .get(&info)
            .expect("All requested workers should be in the worker client registry");
        client.clone()
    }

    async fn from_config<C: Store>(
        config: &C,
        worker: WorkerInfo,
    ) -> Result<ClientRegistry, Error> {
        let mut registry = ClientRegistry::default();
        for (worker_info, addr) in find_peer_workers(config, worker).await? {
            let client = WorkerClient::connect(format!("http://{}", addr)).await?;
            registry.insert(worker_info, client);
        }
        Ok(registry)
    }
}

struct CheckRegistry(Vec<RwLock<Option<Mutex<Vec<ShareCheck>>>>>);

impl CheckRegistry {
    fn new(num_clients: u16) -> CheckRegistry {
        let mut vec = vec![];
        for _ in 0..num_clients {
            vec.push(RwLock::new(Some(Mutex::new(vec![]))));
        }
        CheckRegistry(vec)
    }

    async fn drain(&self, info: ClientInfo) -> Vec<ShareCheck> {
        let mut opt_lock = self.0[info.idx as usize].write().await;
        let vec_lock = opt_lock.take().expect("May only drain once.");
        let mut vec = vec_lock.lock().await;
        Vec::from_iter(vec.drain(..))
    }

    async fn add(&self, info: ClientInfo, value: ShareCheck) -> usize {
        let opt_lock = self.0[info.idx as usize].read().await;
        let vec_lock = opt_lock
            .as_ref()
            .expect("Can only add to client that hasn't had its shares drained.");
        let mut vec = vec_lock.lock().await;
        vec.push(value);
        vec.len()
    }
}

pub struct MyWorker {
    clients_peers: RwLock<HashMap<ClientInfo, Vec<WorkerInfo>>>,
    start_rx: watch::Receiver<bool>,
    clients_rx: watch::Receiver<Option<ClientRegistry>>,
    check_registry: Arc<CheckRegistry>,
    info: WorkerInfo,
}

impl MyWorker {
    fn new(
        start_rx: watch::Receiver<bool>,
        clients_rx: watch::Receiver<Option<ClientRegistry>>,
        num_clients: u16,
        info: WorkerInfo,
    ) -> Self {
        MyWorker {
            clients_peers: RwLock::default(),
            start_rx,
            clients_rx,
            check_registry: Arc::new(CheckRegistry::new(num_clients)),
            info,
        }
    }

    async fn get_peers(&self, info: ClientInfo) -> Result<Vec<SharedClient>, Status> {
        let clients_peers = self.clients_peers.read().await;
        let clients_registry_lock = self.clients_rx.borrow();
        let clients: Vec<SharedClient> = clients_peers
            .get(&info)
            .ok_or_else(|| {
                Status::failed_precondition(format!("Client info {:?} not registered.", info))
            })?
            .clone()
            .into_iter()
            .map(|info| {
                (*clients_registry_lock)
                    .as_ref()
                    .expect(
                        "Client registry should be initialized by the time we get upload requests.",
                    )
                    .get(info)
            })
            .collect();
        Ok(clients)
    }

    fn check_not_started(&self) -> Result<(), Status> {
        let started_lock = self.start_rx.borrow();
        if *started_lock {
            let msg = "Client registration after start time.";
            error!("{}", msg);
            return Err(Status::new(
                tonic::Code::FailedPrecondition,
                msg.to_string(),
            ));
        }
        Ok(())
    }
}

fn expect_field<T>(opt: Option<T>, name: &str) -> Result<T, Status> {
    opt.ok_or_else(|| Status::invalid_argument(format!("{} must be set.", name)))
}

fn gen_audit(share_and_proof: ShareWithProof, num_peers: usize) -> Vec<ShareCheck> {
    debug!(
        "I would be computing an audit for {:?} now.",
        share_and_proof
    );
    return vec![ShareCheck::default(); num_peers + 1];
}

fn verify(shares: &[ShareCheck]) -> bool {
    trace!("Running heavy verification part with shares {:?}.", shares);
    true // TODO(zjn): wire in protocol
}

#[tonic::async_trait]
impl Worker for MyWorker {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        let request = request.into_inner();
        trace!("Request! {:?}", request);

        let client_id = expect_field(request.client_id, "Client ID")?;
        let client_info = ClientInfo::from(client_id.clone());
        let peers: Vec<SharedClient> = self.get_peers(client_info).await?;
        let share_and_proof = expect_field(request.share_and_proof, "ShareWithProof")?;
        let check_registry = self.check_registry.clone();

        spawn(async move {
            let num_peers = peers.len();
            let mut checks = spawn_blocking(move || gen_audit(share_and_proof, num_peers))
                .await
                .expect("Generating audit should not panic.");

            let check = checks.pop().expect("Should have at least one check.");
            check_registry.add(client_info, check).await;
            for (peer, check) in peers.into_iter().zip(checks.into_iter()) {
                let req = Request::new(VerifyRequest {
                    client_id: Some(client_id.clone()),
                    check: Some(check),
                });
                spawn(async move {
                    peer.lock().await.verify(req).await.unwrap();
                });
            }
        });

        Ok(Response::new(UploadResponse {}))
    }

    async fn verify(
        &self,
        request: Request<VerifyRequest>,
    ) -> Result<Response<VerifyResponse>, Status> {
        let request = request.into_inner();
        trace!("Request: {:?}", request);

        // TODO(zjn): check which worker this comes from, don't double-insert
        let client_info = ClientInfo::from(expect_field(request.client_id, "Client ID")?);
        // TODO(zjn): do something less heavy-weight then getting all the peers
        let num_peers = self.get_peers(client_info).await?.len();
        let share = expect_field(request.check, "Check")?;
        let check_count = self.check_registry.add(client_info, share).await;

        if check_count == num_peers + 1 {
            let shares = self.check_registry.drain(client_info).await;
            spawn(async move {
                let verify = spawn_blocking(move || verify(&shares)).await.unwrap();

                if verify {
                    info!("Should combine shares now."); // TODO
                } else {
                    warn!("Didn't verify!.");
                }
            });
        }

        Ok(Response::new(VerifyResponse {}))
    }

    async fn register_client(
        &self,
        request: Request<RegisterClientRequest>,
    ) -> Result<Response<RegisterClientResponse>, Status> {
        self.check_not_started()?;

        let request = request.into_inner();
        let client_id = ClientInfo::from(expect_field(request.client_id, "Client ID")?);

        let shards = request
            .shards
            .into_iter()
            .map(WorkerInfo::from)
            .filter(|&info| info != self.info)
            .collect();
        trace!("Registering client {:?}; shards: {:?}", client_id, shards);
        let mut clients_peers = self.clients_peers.write().await;
        clients_peers.insert(client_id, shards); // TODO(zjn): ensure none; client shouldn't register twice

        let reply = RegisterClientResponse {};
        Ok(Response::new(reply))
    }
}

pub async fn run<C, F>(config: C, info: WorkerInfo, shutdown: F) -> Result<(), Error>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let (start_tx, start_rx) = watch::channel(false);
    let (clients_tx, clients_rx) = watch::channel(None);
    let worker = MyWorker::new(start_rx, clients_rx, 2, info); // TODO(zjn): don't hardcode 2!

    let server = tonic::transport::server::Server::builder()
        .add_service(HealthServer::new(AllGoodHealthServer::default()))
        .add_service(WorkerServer::new(worker))
        .serve_with_shutdown(addr, shutdown);
    let server_task = spawn(server);

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Worker {:?} healthy and serving.", info);
    register(&config, Node::new(info.into(), addr)).await?;

    let start_time = wait_for_start_time_set(&config).await.unwrap();
    spawn(delay_until(start_time).then(|_| async move { start_tx.broadcast(true) }));

    let client_registry = ClientRegistry::from_config(&config, info).await?;
    clients_tx
        .broadcast(Some(client_registry))
        .or_else(|_| Err("Error sending client registry."))?;

    server_task.await??;
    info!("Worker shutting down.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, experiment::Experiment, services::Group};
    use std::ops::Deref;

    const NUM_CLIENTS: u16 = 10;
    const NUM_SHARES: u16 = 100;

    #[tokio::test]
    async fn test_check_registry_empty() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);

        for client in clients {
            let shares = reg.drain(client).await;
            assert!(shares.is_empty());
        }
    }

    #[tokio::test]
    async fn test_check_registry_put_shares() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);
        let expected_shares = vec![ShareCheck::default(); NUM_SHARES as usize];

        for client in &clients {
            for (idx, share) in expected_shares.iter().enumerate() {
                assert_eq!(reg.add(*client, share.clone()).await, idx + 1);
            }
        }

        for client in clients {
            assert_eq!(reg.drain(client).await, expected_shares);
        }
    }

    #[should_panic]
    #[tokio::test]
    async fn test_check_registry_drain_twice_panics() {
        let mut clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);

        for client in &clients {
            reg.drain(*client).await;
        }

        reg.drain(clients.pop().unwrap()).await;
    }

    #[should_panic]
    #[test]
    fn test_client_registry_get_missing() {
        let reg = ClientRegistry::default();
        reg.get(WorkerInfo::new(Group::new(0), 0));
    }

    #[tokio::test]
    async fn test_client_registry_get_and_insert() {
        let mut reg = ClientRegistry::default();
        let info = WorkerInfo::new(Group::new(0), 0);
        let client = WorkerClient::connect("http://www.example.com:80")
            .await
            .unwrap();

        reg.insert(info, client);
        let client_mutex = reg.get(info);
        let client_lock = client_mutex.lock().await;
        let _: &WorkerClient<Channel> = client_lock.deref();
        // if we got /any/ client back, it's okay
    }

    #[tokio::test]
    async fn test_find_peer_workers_empty() {
        let config = config::factory::from_string("").unwrap();
        let info = WorkerInfo::new(Group::new(0), 0);

        let peers = find_peer_workers(&config, info).await.unwrap();

        assert_eq!(peers.len(), 0);
    }

    #[tokio::test]
    async fn test_find_peer_workers() {
        let config = config::factory::from_string("").unwrap();

        let experiment = Experiment::new(2, 2, 2);
        for service in experiment.iter_services() {
            let node = Node::new(service, "127.0.0.1:22".parse().unwrap());
            register(&config, node).await.unwrap()
        }

        let worker = experiment
            .iter_services()
            .filter_map(|service| match service {
                Service::Worker(info) => Some(info),
                _ => None,
            })
            .next()
            .unwrap();

        let peers = find_peer_workers(&config, worker).await.unwrap();
        assert_eq!(peers.len(), 3);
    }
}
