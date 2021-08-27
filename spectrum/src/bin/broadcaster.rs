use clap::{crate_authors, crate_version, ArgGroup, Clap};
use futures::prelude::*;
use rand::{thread_rng, Rng};
use spectrum_impl::{
    cli, client, config, experiment, protocols::wrapper::ChannelKeyWrapper, services::ClientInfo,
};
use spectrum_primitives::Bytes;
use std::convert::TryFrom;
use std::fs::File;
use std::io::Read;
use tokio::signal::ctrl_c;

/// Run a Spectrum broadcasting client.
///
/// Broadcasters send a message, authorized by a hidden channel key.
///
/// They also read a message so that network traffic looks identical.
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
    client: BroadcasterArgs,
}

#[derive(Clap)]
#[clap(group = ArgGroup::new("message").required(true))]
struct BroadcasterArgs {
    /// The message to broadcast
    #[clap(long = "message", group = "message")]
    msg: Option<String>,

    /// File containing the message to broadcast, or `-` for STDIN.
    #[clap(long = "message-file", group = "message")]
    msg_file: Option<String>,

    /// File containing the broadcast key, serialized to JSON.
    #[clap(long, required = true)]
    key_file: String,
}

impl TryFrom<BroadcasterArgs> for ClientInfo {
    type Error = String;

    fn try_from(args: BroadcasterArgs) -> Result<Self, Self::Error> {
        let msg = if let Some(msg) = args.msg {
            Bytes::from(Vec::from(msg.as_bytes()))
        } else if let Some(msg_file) = args.msg_file {
            let msg_file_reader = File::open(&msg_file).map_err(|e| e.to_string())?;
            Bytes::from(
                msg_file_reader
                    .bytes()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            return Err(
                "Invalid BroadcasterArgs: at least one of `msg`, `msg_file` must be `Some()`."
                    .to_string(),
            );
        };
        let key_file = args.key_file;
        let key_file_reader = File::open(&key_file).map_err(|e| e.to_string())?;
        let key: ChannelKeyWrapper = serde_json::from_reader(key_file_reader)
            .map_err(|e| format!("Could not read key file [{}]: {}", key_file, e.to_string()))?;
        // -1 because the CLI needs non-zero or it thinks we didn't supply it
        // from environment variable
        Ok(Self::new_broadcaster(thread_rng().gen(), msg, key))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let info = ClientInfo::try_from(args.client)?;
    client::viewer::run(
        config,
        experiment.get_protocol().clone(),
        info,
        experiment.hammer,
        None,
        ctrl_c().map(|_| ()),
    )
    .await
}
