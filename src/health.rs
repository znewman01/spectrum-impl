use crate::config::store::Error;
use log::debug;
use std::time::Duration;
use tokio::time::delay_for;
use tonic::{Request, Response, Status};

pub mod spectrum {
    tonic::include_proto!("grpc.health.v1");
}

pub use spectrum::{
    health_check_response::ServingStatus,
    health_client::HealthClient,
    health_server::{Health, HealthServer},
    HealthCheckRequest, HealthCheckResponse,
};

const RETRY_DELAY: Duration = Duration::from_millis(50);
const RETRY_ATTEMPTS: usize = 10;

#[derive(Default)]
pub struct AllGoodHealthServer {}

#[tonic::async_trait]
impl Health for AllGoodHealthServer {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let reply = HealthCheckResponse {
            status: ServingStatus::Serving as i32,
        };
        Ok(Response::new(reply))
    }
}

async fn is_healthy(addr: String) -> Result<bool, Error> {
    let mut client = HealthClient::connect(addr)
        .await
        .map_err(|err| err.to_string())?;
    let req = Request::new(HealthCheckRequest {
        service: "".to_string(),
    });
    let response = client.check(req).await.map_err(|err| err.to_string())?;
    Ok(response.into_inner().status == ServingStatus::Serving as i32)
}

pub async fn wait_for_health_helper(
    addr: String,
    delay: Duration,
    attempts: usize,
) -> Result<(), Error> {
    for _ in 0..attempts {
        match is_healthy(addr.clone()).await {
            Ok(response) => {
                if response {
                    return Ok(());
                }
            }
            Err(err) => {
                debug!("Error checking health: {}", err);
            }
        }
        delay_for(delay).await;
    }
    Err(Error::new(&format!(
        "Service not healthy after {} attempts",
        attempts
    )))
}

pub async fn wait_for_health(addr: String) -> Result<(), Error> {
    wait_for_health_helper(addr, RETRY_DELAY, RETRY_ATTEMPTS).await
}
