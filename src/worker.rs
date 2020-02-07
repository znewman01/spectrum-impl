use crate::proto::{
    worker_client::WorkerClient,
    worker_server::{Worker, WorkerServer},
    RegisterClientRequest, RegisterClientResponse, UploadRequest, UploadResponse, VerifyRequest,
    VerifyResponse,
};
use crate::{
    config::store::{Error, Store},
    net::get_addr,
    services::{
        discovery::{register, resolve_all, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, Service, WorkerInfo,
    },
};

use futures::Future;
use log::{debug, error, info, trace};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};
use tonic::{transport::Channel, Request, Response, Status};

type SharedClient = Arc<Mutex<WorkerClient<Channel>>>;
type ClientRegistry = HashMap<WorkerInfo, SharedClient>;

pub struct MyWorker {
    clients_peers: RwLock<HashMap<ClientInfo, Vec<WorkerInfo>>>,
    start_rx: watch::Receiver<bool>,
    clients_rx: watch::Receiver<Option<ClientRegistry>>,
}

impl MyWorker {
    fn new(
        start_rx: watch::Receiver<bool>,
        clients_rx: watch::Receiver<Option<ClientRegistry>>,
    ) -> Self {
        MyWorker {
            clients_peers: RwLock::new(HashMap::new()),
            start_rx,
            clients_rx,
        }
    }
}

#[tonic::async_trait]
impl Worker for MyWorker {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        let request = request.into_inner();
        debug!("Request! {:?}", request);

        let client_id = request
            .client_id
            .ok_or_else(|| Status::invalid_argument("Client ID must be set."))?;
        let client_info = ClientInfo::from(client_id.clone());
        let clients_peers = self.clients_peers.read().await;
        let clients_registry_lock = self.clients_rx.borrow();
        let peers: Vec<Arc<Mutex<WorkerClient<_>>>> = clients_peers
            .get(&client_info)
            .ok_or_else(|| {
                Status::failed_precondition(format!("Client info {:?} not registered.", client_info))
            })?
            .clone()
            .iter()
            .map(|info| {
                (*clients_registry_lock).as_ref()
                    .expect("Client registry should be initialized by the time we get cupload requests.")
                    .get(info)
                    .expect("All requested workers should be in the worker client registry")
                    .clone()
            })
            .collect();
        let share_and_proof = request.share_and_proof;

        tokio::spawn(async move {
            let num_peers = peers.len();
            let checks = tokio::task::spawn_blocking(move || {
                debug!(
                    "I would be computing an audit for {:?} now.",
                    share_and_proof
                );
                return vec![0u32; num_peers];
            })
            .await
            .unwrap();

            assert_eq!(checks.len(), num_peers);
            for (peer, check) in peers.into_iter().zip(checks.into_iter()) {
                let client_id = client_id.clone();
                tokio::spawn(async move {
                    debug!("I would be sending check {:?} now.", check);
                    let req = tonic::Request::new(VerifyRequest {
                        client_id: Some(client_id),
                        check: None,
                    });
                    peer.lock().await.verify(req).await.unwrap();
                });
            }
        });

        let reply = UploadResponse {};
        Ok(Response::new(reply))
    }

    async fn verify(
        &self,
        request: Request<VerifyRequest>,
    ) -> Result<Response<VerifyResponse>, Status> {
        trace!("Request: {:?}", request.into_inner());

        // TODO(zjn): implement me
        let reply = VerifyResponse {};
        Ok(Response::new(reply))
    }

    async fn register_client(
        &self,
        request: Request<RegisterClientRequest>,
    ) -> Result<Response<RegisterClientResponse>, Status> {
        {
            let started_lock = self.start_rx.borrow();
            if *started_lock {
                let msg = "Client registration after start time.";
                error!("{}", msg);
                return Err(Status::new(
                    tonic::Code::FailedPrecondition,
                    msg.to_string(),
                ));
            }
        }

        let request = request.into_inner();
        let client_id = request
            .client_id
            .map(ClientInfo::from)
            .ok_or_else(|| Status::invalid_argument("Client ID must be set."))?;

        let shards = request.shards.into_iter().map(WorkerInfo::from).collect();
        trace!("Registering client {:?}", client_id);
        trace!("Client shards: {:?}", shards);
        let mut clients_peers = self.clients_peers.write().await;
        clients_peers.insert(client_id, shards); // TODO(zjn): ensure none; client shouldn't register twice

        let reply = RegisterClientResponse {};
        Ok(Response::new(reply))
    }
}

pub async fn run<C, F>(
    config: C,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let (start_tx, start_rx) = watch::channel(false);
    let (clients_tx, clients_rx) = watch::channel(None);
    let worker = MyWorker::new(start_rx, clients_rx);

    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(WorkerServer::new(worker))
            .serve_with_shutdown(addr, shutdown),
    );

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Worker {:?} healthy and serving.", info);

    let node = Node::new(info.into(), addr);
    register(&config, node).await?;
    debug!("Registered with config server.");

    let start_time = wait_for_start_time_set(&config).await?;

    let mut clients_registry = HashMap::new();
    for node in resolve_all(&config).await? {
        if let Service::Worker(info) = node.service {
            let client = WorkerClient::connect(format!("http://{}", node.addr)).await?;
            clients_registry.insert(info, Arc::new(Mutex::new(client)));
        }
    }
    if clients_tx.broadcast(Some(clients_registry)).is_err() {
        return Err(Box::new(Error::new(
            "Failed to broadcast clients_registry.",
        )));
    }

    delay_until(start_time).await;
    start_tx.broadcast(true)?;

    server_task.await??;
    info!("Worker shutting down.");

    Ok(())
}
