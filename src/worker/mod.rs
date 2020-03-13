use crate::proto::{
    worker_server::{Worker, WorkerServer},
    RegisterClientRequest, RegisterClientResponse, UploadRequest, UploadResponse, VerifyRequest,
    VerifyResponse,
};
use crate::{
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::accumulator::Accumulator,
    protocols::{
        insecure::{InsecureAuditShare, InsecureWriteToken},
        Protocol,
    },
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, WorkerInfo,
    },
};

use futures::prelude::*;
use log::{info, trace, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{
    spawn,
    sync::{watch, RwLock},
    task::spawn_blocking,
};
use tonic::{Request, Response, Status};

mod audit_registry;
mod client_registry;

use audit_registry::AuditRegistry;
use client_registry::{ClientRegistry, SharedClient};

type Error = Box<dyn std::error::Error + Sync + Send>;

pub struct MyWorker {
    clients_peers: RwLock<HashMap<ClientInfo, Vec<WorkerInfo>>>,
    start_rx: watch::Receiver<bool>,
    clients_rx: watch::Receiver<Option<ClientRegistry>>,
    audit_registry: Arc<AuditRegistry<InsecureAuditShare, InsecureWriteToken>>,
    accumulator: Arc<Accumulator<Vec<u8>>>,
    experiment: Experiment,
    _info: WorkerInfo, // might use for logging eventually
}

impl MyWorker {
    fn new(
        start_rx: watch::Receiver<bool>,
        clients_rx: watch::Receiver<Option<ClientRegistry>>,
        experiment: Experiment,
        info: WorkerInfo,
    ) -> Self {
        MyWorker {
            clients_peers: RwLock::default(),
            start_rx,
            clients_rx,
            audit_registry: Arc::new(AuditRegistry::new(experiment.clients)),
            accumulator: Arc::new(Accumulator::new(vec![0u8; experiment.channels])),
            experiment,
            _info: info,
        }
    }

    async fn get_peers(&self, info: ClientInfo) -> Result<Vec<SharedClient>, Status> {
        let clients_peers = self.clients_peers.read().await;
        let clients_registry_lock = self.clients_rx.borrow();
        let clients = clients_peers
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
            .collect::<Result<_, _>>()?;
        Ok(clients)
    }

    fn check_not_started(&self) -> Result<(), Status> {
        let started_lock = self.start_rx.borrow();
        if *started_lock {
            Err(Status::failed_precondition(
                "Client registration after start time.",
            ))
        } else {
            Ok(())
        }
    }
}

fn expect_field<T>(opt: Option<T>, name: &str) -> Result<T, Status> {
    opt.ok_or_else(|| Status::invalid_argument(format!("{} must be set.", name)))
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
        let peers: Vec<SharedClient> = self.get_peers(client_info.clone()).await?;
        let write_token: InsecureWriteToken =
            expect_field(request.write_token, "Write Token")?.into();
        let protocol = self.experiment.get_protocol();
        let keys = self.experiment.get_keys();
        let audit_registry = self.audit_registry.clone();

        spawn(async move {
            // Avoid cloning write token by passing it back
            let (audit_shares, write_token) = spawn_blocking(move || {
                let audit_shares = protocol.gen_audit(&keys, &write_token);
                (audit_shares, write_token)
            })
            .await
            .expect("Generating audit should not panic.");
            audit_registry.init(client_info, write_token).await;

            for (peer, audit_share) in peers.into_iter().zip(audit_shares.into_iter()) {
                let req = Request::new(VerifyRequest {
                    client_id: Some(client_id.clone()),
                    audit_share: Some(audit_share.into()),
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
        let total_clients = {
            let lock = self.clients_peers.read().await;
            lock.len()
        };
        let share = expect_field(request.audit_share, "Audit Share")?;
        let check_count = self
            .audit_registry
            .add(client_info.clone(), share.into())
            .await;
        let accumulator: Arc<Accumulator<Vec<u8>>> = self.accumulator.clone();
        let protocol = self.experiment.get_protocol();

        if check_count == self.experiment.groups as usize {
            let (token, shares) = self.audit_registry.drain(client_info).await;
            spawn(async move {
                let verify = spawn_blocking(move || protocol.check_audit(shares))
                    .await
                    .unwrap();

                if verify {
                    let data = protocol.to_accumulator(token);
                    if accumulator.accumulate(data).await == total_clients {
                        let share = accumulator.get().await;
                        info!("Should forward to leader now! {:?}", share);
                    };
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
        let client_info = ClientInfo::from(expect_field(request.client_id, "Client ID")?);

        let shards = request
            .shards
            .into_iter()
            .map(WorkerInfo::from)
            // .filter(|&info| info != self.info)
            .collect();
        trace!("Registering client {:?}; shards: {:?}", client_info, shards);
        let mut clients_peers = self.clients_peers.write().await;
        if clients_peers.insert(client_info.clone(), shards).is_some() {
            warn!("Client registered twice: {:?}", client_info);
        }

        let reply = RegisterClientResponse {};
        Ok(Response::new(reply))
    }
}

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Error>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let (start_tx, start_rx) = watch::channel(false);
    let (clients_tx, clients_rx) = watch::channel(None);
    let worker = MyWorker::new(start_rx, clients_rx, experiment, info);

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

    let client_registry = ClientRegistry::from_config(&config).await?;
    clients_tx
        .broadcast(Some(client_registry))
        .or_else(|_| Err("Error sending client registry."))?;

    server_task.await??;
    info!("Worker shutting down.");
    Ok(())
}
