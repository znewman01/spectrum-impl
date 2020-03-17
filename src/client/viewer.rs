use crate::proto::UploadRequest;
use crate::{
    client::connections,
    config,
    experiment::Experiment,
    protocols::Protocol,
    services::{
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo,
    },
};
use config::store::Store;
use futures::prelude::*;
use log::{debug, info, trace};

type TokioError = Box<dyn std::error::Error + Sync + Send>;

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    info: ClientInfo,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Client starting");
    let start_time = wait_for_start_time_set(&config).await?;
    debug!("Received configuration from configuration server; initializing.");

    let mut clients = connections::connect_and_register(&config, info.clone()).await?;
    let protocol = experiment.get_protocol();
    let client_id = info.to_proto(); // before we move info
    let write_tokens = match info.broadcast {
        Some((msg, key)) => protocol.broadcast(vec![msg], key),
        None => protocol.null_broadcast(),
    };

    delay_until(start_time).await;

    for (client, write_token) in clients.iter_mut().zip(write_tokens) {
        let req = tonic::Request::new(UploadRequest {
            client_id: Some(client_id.clone()),
            write_token: Some(write_token.into()),
        });
        trace!("About to send upload request.");
        let response = client.upload(req).await?;
        debug!("RESPONSE={:?}", response.into_inner());
    }

    shutdown.await;

    Ok(())
}
