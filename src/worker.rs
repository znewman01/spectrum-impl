use crate::{
    config::store::Store,
    net::get_addr,
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        Group, WorkerInfo,
    },
};
use futures::Future;
use log::{debug, error, info, trace};
use std::collections::HashMap;
use tokio::sync::{watch, RwLock};
use tonic::{Request, Response, Status};

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{
    worker_server::{Worker, WorkerServer},
    RegisterClientRequest, RegisterClientResponse, UploadRequest, UploadResponse, VerifyRequest,
    VerifyResponse,
};

pub struct MyWorker {
    // TODO(zjn): replace () with ClientInfo
    clients_peers: RwLock<HashMap<(), Vec<WorkerInfo>>>,
    start_rx: watch::Receiver<bool>,
}

impl MyWorker {
    fn new(start_rx: watch::Receiver<bool>) -> Self {
        MyWorker {
            clients_peers: RwLock::new(HashMap::new()),
            start_rx,
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
            .map(|_| ()) // TODO(zjn): replace with ClientInfo
            .ok_or_else(|| Status::invalid_argument("Client ID must be set."))?;
        let clients_peers = self.clients_peers.read().await;
        let peers: Vec<WorkerInfo> = clients_peers
            .get(&client_id)
            .ok_or_else(|| Status::failed_precondition("Client ID not registered."))?
            .clone();
        let share_and_proof = request.share_and_proof;

        tokio::spawn(async move {
            let num_peers = peers.len();
            let checks = tokio::task::spawn_blocking(move || {
                debug!(
                    "I would be computing an audit for {:?} now.",
                    share_and_proof
                );
                return vec![num_peers];
            })
            .await
            .unwrap();

            assert_eq!(checks.len(), num_peers);
            for (peer, check) in peers.into_iter().zip(checks.into_iter()) {
                tokio::spawn(async move {
                    debug!(
                        "I would be sending check {:?} to peer {:?} now.",
                        check, peer
                    );
                });
            }
        });

        let reply = UploadResponse {};
        Ok(Response::new(reply))
    }

    async fn verify(
        &self,
        _request: Request<VerifyRequest>,
    ) -> Result<Response<VerifyResponse>, Status> {
        error!("Not implemented.");
        Err(Status::unimplemented("Not implemented"))
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
        trace!("Registering client {:?}", request.client_id);

        let client_id = request
            .client_id
            .map(|_| ()) // TODO(zjn): replace with ClientInfo
            .ok_or_else(|| Status::invalid_argument("Client ID must be set."))?;

        let mut clients_peers = self.clients_peers.write().await;
        clients_peers.insert(
            client_id,
            request
                .shards
                .iter()
                .map(|_shard| WorkerInfo::new(Group::new(0), 0)) // TODO(zjn): create valid WorkerId
                .collect(),
        ); // TODO(zjn): ensure none; client shouldn't register twice

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
    let worker = MyWorker::new(start_rx);

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
    delay_until(start_time).await;
    start_tx.broadcast(true)?;

    server_task.await??;
    info!("Worker shutting down.");

    Ok(())
}
