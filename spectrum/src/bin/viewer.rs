use std::iter::repeat_with;

use clap::{crate_authors, crate_version, Clap};
use futures::stream::{FuturesUnordered, StreamExt};
use spectrum_impl::{cli, client, config, experiment, services::ClientInfo};

/// Run a Spectrum viewing client.
///
/// Viewers do two things:
///
/// - send cover traffic to worker nodes
///
/// - read the recovered message
///
/// Use `$SPECTRUM_CONFIG_SERVER=etcd://127.0.0.1:8000` to point to an etcd
/// instance, and the client will pick up the experiment configuration from
/// there.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    logs: cli::LogArgs,
    #[clap(flatten)]
    client: ViewerArgs,
    /// Run this many threads in parallel.
    #[clap(long, env = "SPECTRUM_VIEWER_THREADS", default_value = "1")]
    threads: u16,
}

#[derive(Clap)]
struct ViewerArgs {
    /// The index of this viewer among all clients.
    #[clap(long = "index", env = "SPECTRUM_VIEWER_INDEX")]
    idx: u128,
}

impl From<ViewerArgs> for ClientInfo {
    fn from(args: ViewerArgs) -> Self {
        Self::new(args.idx - 1)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let info = ClientInfo::from(args.client);

    repeat_with(|| {
        let protocol = experiment.get_protocol().clone();
        let info = info.clone();
        let config = config.clone();
        tokio::spawn(async move {
            client::viewer::run(config, protocol, info, futures::future::ready(())).await
        })
    })
    .take(args.threads.into())
    .collect::<FuturesUnordered<_>>()
    .map(|r: Result<Result<(), _>, _>| match r {
        Ok(Ok(())) => Ok::<(), Box<dyn std::error::Error + Sync + Send>>(()),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(err.into()),
    })
    .collect::<Vec<Result<(), _>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, _>>()
    .map(|_| ())
}
