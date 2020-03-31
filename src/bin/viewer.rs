use clap::{crate_authors, crate_version, Clap};
use futures::prelude::*;
use spectrum_impl::{cli, client, config, experiment, services::ClientInfo};
use tokio::signal::ctrl_c;

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
}

#[derive(Clap)]
struct ViewerArgs {
    /// The index of this viewer among all clients.
    #[clap(long = "index")]
    idx: u16,
}

impl From<ViewerArgs> for ClientInfo {
    fn from(args: ViewerArgs) -> Self {
        Self::new(args.idx)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let info = ClientInfo::from(args.client);
    client::viewer::run(
        config,
        experiment.get_protocol().clone(),
        info,
        ctrl_c().map(|_| ()),
    )
    .await
}
