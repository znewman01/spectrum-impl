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
use spectrum_primitives::Bytes;

use config::store::Store;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use log::{debug, error, info, trace, warn};
use tokio::time::sleep;
use tonic::transport::Certificate;

use std::fmt;
use std::time::Duration;
use std::{
    convert::{TryFrom, TryInto},
    time::Instant,
};

type TokioError = Box<dyn std::error::Error + Sync + Send>;

async fn inner_run<C, F, P>(
    config: C,
    protocol: P,
    info: ClientInfo,
    hammer: bool,
    cert: Option<Certificate>,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
    P: Protocol,
    P::ChannelKey: TryFrom<ChannelKeyWrapper>,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: fmt::Debug,
    P::WriteToken: Into<proto::WriteToken>
        + fmt::Debug
        + Send
        + Clone
        + TryFrom<proto::WriteToken>
        + PartialEq,
    Bytes: TryInto<P::Accumulator> + TryFrom<P::Accumulator>,
    <Bytes as TryInto<P::Accumulator>>::Error: fmt::Debug,
    <Bytes as TryFrom<P::Accumulator>>::Error: fmt::Debug,
{
    info!("Client starting");
    let start_time = wait_for_start_time_set(&config).await?;
    debug!("Received configuration from configuration server; initializing.");

    let clients: Vec<_> = connections::connect_and_register(&config, info.clone(), cert).await?;
    let client_id = info.to_proto(); // before we move info
    let mut write_tokens = match info.broadcast {
        Some((msg, key)) => {
            info!("Broadcaster about to send write token.");
            debug!("Write token: msg.len()={}, key={:?}", msg.len(), key);
            protocol.broadcast(
                msg.try_into().unwrap(),
                info.idx.try_into().expect("idx should be small"),
                key.try_into().unwrap(),
            )
        }
        None => protocol.cover(),
    };

    delay_until(start_time).await;
    const MAX_JITTER_MILLIS: u64 = 100;
    let jitter = Duration::from_millis(rand::random::<u64>() % MAX_JITTER_MILLIS);
    sleep(jitter).await;
    debug!("Client detected start time ready.");

    loop {
        clients
            .iter()
            .cloned()
            .zip(write_tokens.into_iter())
            .map(|(mut client, write_token)| {
                let client_id = client_id.clone();
                let write_token = write_token.into();
                tokio::spawn(async move {
                    let response;
                    let start_time = Instant::now();
                    loop {
                        let req = tonic::Request::new(UploadRequest {
                            client_id: Some(client_id.clone()),
                            write_token: Some(write_token.clone()),
                        });
                        trace!("About to send upload request.");
                        {
                            match client.upload(req).await {
                                Ok(r) => {
                                    response = r;
                                    break;
                                }
                                Err(err) => warn!("Error, trying again: {}", err),
                            };
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                    info!("Request took {}ms.", start_time.elapsed().as_millis());
                    debug!("RESPONSE={:?}", response.into_inner());
                })
            })
            .collect::<FuturesUnordered<_>>()
            .inspect_err(|err| error!("{:?}", err))
            .try_collect::<Vec<_>>()
            .await
            .expect("tokio spawn should succeed");
        if !hammer {
            break;
        }
        write_tokens = protocol.cover();
    }

    shutdown.await;

    Ok(())
}

pub async fn run<C, F>(
    config: C,
    protocol: ProtocolWrapper,
    info: ClientInfo,
    hammer: bool,
    cert: Option<Certificate>,
    shutdown: F,
) -> Result<(), TokioError>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper::Secure(protocol) => {
            inner_run(config, protocol, info, hammer, cert, shutdown).await?;
        }
        ProtocolWrapper::SecureMultiKey(protocol) => {
            inner_run(config, protocol, info, hammer, cert, shutdown).await?;
        }
        ProtocolWrapper::Insecure(protocol) => {
            inner_run(config, protocol, info, hammer, cert, shutdown).await?;
        }
    }
    Ok(())
}
