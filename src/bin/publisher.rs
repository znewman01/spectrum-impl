use futures::prelude::*;
use spectrum_impl::{config, experiment, publisher, services::PublisherInfo};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    // TODO(zjn): construct from environment/args
    let info = PublisherInfo::new();
    publisher::run(
        config,
        experiment.get_protocol().clone(),
        info,
        publisher::NoopRemote,
        ctrl_c().map(|_| ()),
    )
    .await
}
