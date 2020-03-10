use crate::proto::{
    worker_server::{Worker, WorkerServer},
    RegisterClientRequest, RegisterClientResponse, AuditShare, WriteToken, UploadRequest,
    UploadResponse, VerifyRequest, VerifyResponse,
};
use crate::{
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::accumulator::Accumulator,
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, WorkerInfo,
    },
};

use futures::prelude::*;
use log::{debug, info, trace, warn};
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
    audit_registry: Arc<AuditRegistry>,
    accumulator: Arc<Accumulator<Vec<Option<String>>>>,
    experiment: Experiment,
    info: WorkerInfo,
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
            accumulator: Arc::new(Accumulator::new(vec![None; experiment.channels])),
            experiment,
            info,
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

fn gen_audit(share_and_proof: WriteToken, num_peers: usize) -> Vec<AuditShare> {
    debug!(
        "I would be computing an audit for {:?} now.",
        share_and_proof
    );
    return vec![AuditShare::default(); num_peers + 1];
}

fn verify(shares: &[AuditShare]) -> bool {
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
        let write_token = expect_field(request.write_token, "Write Token")?;
        let audit_registry = self.audit_registry.clone();

        spawn(async move {
            let num_peers = peers.len();
            let mut audit_shares = spawn_blocking(move || gen_audit(write_token, num_peers))
                .await
                .expect("Generating audit should not panic.");

            let audit_share = audit_shares.pop().expect("Should have at least one audit share.");
            audit_registry.add(client_info, audit_share).await;
            for (peer, check) in peers.into_iter().zip(audit_shares.into_iter()) {
                let req = Request::new(VerifyRequest {
                    client_id: Some(client_id.clone()),
                    audit_share: Some(check),
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
        let num_clients = {
            let lock = self.clients_peers.read().await;
            lock.len()
        };
        let share = expect_field(request.audit_share, "Audit Share")?;
        let check_count = self.audit_registry.add(client_info, share).await;
        let accumulator = self.accumulator.clone();
        let num_channels = self.experiment.channels;

        if check_count == self.experiment.groups as usize {
            let shares = self.audit_registry.drain(client_info).await;
            spawn(async move {
                let verify = spawn_blocking(move || verify(&shares)).await.unwrap();

                if verify {
                    let data = vec![None; num_channels]; // TODO(zjn): pull out of something
                    if accumulator.accumulate(data).await == num_clients {
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
            .filter(|&info| info != self.info)
            .collect();
        trace!("Registering client {:?}; shards: {:?}", client_info, shards);
        let mut clients_peers = self.clients_peers.write().await;
        if clients_peers.insert(client_info, shards).is_some() {
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

    let client_registry = ClientRegistry::from_config(&config, info).await?;
    clients_tx
        .broadcast(Some(client_registry))
        .or_else(|_| Err("Error sending client registry."))?;

    server_task.await??;
    info!("Worker shutting down.");
    Ok(())
}
