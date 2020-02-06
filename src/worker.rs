use crate::{
    config::store::Store,
    experiment::filter_peers,
    net::get_addr,
    services::{
        discovery::{register, resolve_all, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::wait_for_start_time_set,
        WorkerInfo,
    },
};
use futures::Future;
use log::{debug, error, info, trace};
use tokio::sync::watch;
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
    peers_rx: watch::Receiver<Option<Vec<Node>>>,
}

impl MyWorker {
    fn new(peers_rx: watch::Receiver<Option<Vec<Node>>>) -> Self {
        MyWorker { peers_rx }
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

        // read lock; ok to hold onto indefinitely because we only send one value in this channel
        let peers_lock = self.peers_rx.borrow();
        let peers = peers_lock
            .as_ref()
            .expect("By the time Worker receives requests, it should know its peers.")
            .clone();

        tokio::spawn(async move {
            let num_peers = peers.len();
            let checks = tokio::task::spawn_blocking(move || {
                debug!("I would be computing an audit for {:?} now.", request);
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
        Err(Status::new(
            tonic::Code::Unimplemented,
            "Not implemented".to_string(),
        ))
    }

    async fn register_client(
        &self,
        _request: Request<RegisterClientRequest>,
    ) -> Result<Response<RegisterClientResponse>, Status> {
        error!("Not implemented.");
        Err(Status::new(
            tonic::Code::Unimplemented,
            "Not implemented".to_string(),
        ))
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

    let (peer_tx, peer_rx) = watch::channel(None);
    let worker = MyWorker::new(peer_rx);

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

    wait_for_start_time_set(&config).await?;
    let peers = filter_peers(info, resolve_all(&config).await?);
    peer_tx.broadcast(Some(peers))?;

    server_task.await??;
    info!("Worker shutting down.");

    Ok(())
}
