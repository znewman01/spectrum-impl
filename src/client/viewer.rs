use crate::proto::{self, UploadRequest};
use crate::{
    client::connections,
    config,
    protocols::{wrapper::ChannelKeyWrapper, wrapper::ProtocolWrapper2, Protocol},
    services::{
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo,
    },
};
use config::store::Store;
use futures::prelude::*;
use log::{debug, info, trace};
use std::convert::{TryFrom, TryInto};
use std::fmt;

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
        Some((msg, key)) => protocol.broadcast(msg, key.try_into().unwrap()),
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

pub async fn run<C, F>(
    config: C,
    protocol: ProtocolWrapper2,
    info: ClientInfo,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper2::Secure(protocol) => {
            inner_run(config, protocol, info, shutdown).await?;
        }
        ProtocolWrapper2::Insecure(protocol) => {
            inner_run(config, protocol, info, shutdown).await?;
        }
    }
    Ok(())
}
