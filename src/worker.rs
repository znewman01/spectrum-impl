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
    UploadRequest, UploadResponse, VerifyRequest, VerifyResponse,
};

pub struct MyWorker {
    peers_rx: watch::Receiver<Option<Vec<Node>>>,
}

impl MyWorker {
    fn new(peers_rx: watch::Receiver<Option<Vec<Node>>>) -> Self {
        MyWorker {
            peers_rx
        }
    }
}

#[tonic::async_trait]
impl Worker for MyWorker {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        debug!("Request! {:?}", request.into_inner());

        {
            let peers_ref = self.peers_rx.borrow();
            let peers = peers_ref.as_ref().expect("By the time Worker receieves requests, it should know its peers.");
            debug!("I have peers! {:?}", peers);
        }

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
