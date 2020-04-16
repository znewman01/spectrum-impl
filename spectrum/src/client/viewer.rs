use crate::proto::{self, UploadRequest};
use crate::{
    client::connections,
    config,
    protocols::{wrapper::ChannelKeyWrapper, wrapper::ProtocolWrapper, Protocol},
    services::{
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo,
    },
};

use config::store::Store;
use futures::prelude::*;
use log::{debug, info, trace, warn};
use tokio::time::delay_for;

use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::time::Duration;

type TokioError = Box<dyn std::error::Error + Sync + Send>;

async fn inner_run<C, F, P>(
    config: C,
    protocol: P,
    info: ClientInfo,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
    P: Protocol,
    P::ChannelKey: TryFrom<ChannelKeyWrapper>,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: fmt::Debug,
    P::WriteToken: Into<proto::WriteToken>,
{
    info!("Client starting");
    let start_time = wait_for_start_time_set(&config).await?;
    debug!("Received configuration from configuration server; initializing.");

    let mut clients = connections::connect_and_register(&config, info.clone()).await?;
    let client_id = info.to_proto(); // before we move info
    let write_tokens = match info.broadcast {
        Some((msg, key)) => {
            info!("Broadcaster about to send write token.");
            debug!("Write token: msg.len()={}, key={:?}", msg.len(), key);
            protocol.broadcast(msg, key.try_into().unwrap())
        }
        None => protocol.null_broadcast(),
    };

    delay_until(start_time).await;
    const MAX_JITTER_MILLIS: u64 = 100;
    let jitter = Duration::from_millis(rand::random::<u64>() % MAX_JITTER_MILLIS);
    delay_for(jitter).await;
    debug!("Client detected start time ready.");

    for (client, write_token) in clients.iter_mut().zip(write_tokens) {
        let response;
        let write_token = write_token.into();
        loop {
            let req = tonic::Request::new(UploadRequest {
                client_id: Some(client_id.clone()),
                write_token: Some(write_token.clone()),
            });
            trace!("About to send upload request.");
            match client.upload(req).await {
                Ok(r) => {
                    response = r;
                    break;
                }
                Err(err) => warn!("Error, trying again: {}", err),
            };
            delay_for(Duration::from_millis(50)).await;
        }
        debug!("RESPONSE={:?}", response.into_inner());
    }

    shutdown.await;

    Ok(())
}

pub async fn run<C, F>(
    config: C,
    protocol: ProtocolWrapper,
    info: ClientInfo,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper::Secure(protocol) => {
            inner_run(config, protocol, info, shutdown).await?;
        }
        ProtocolWrapper::SecureMultiKey(protocol) => {
            inner_run(config, protocol, info, shutdown).await?;
        }
        ProtocolWrapper::Insecure(protocol) => {
            inner_run(config, protocol, info, shutdown).await?;
        }
    }
    Ok(())
}
