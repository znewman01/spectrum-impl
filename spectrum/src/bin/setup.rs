use spectrum::cli;
use spectrum::config;
use spectrum::experiment::{write_to_store, Experiment};

use clap::{crate_authors, crate_version, Clap};

// use std::fs::File;

/// Spectrum -- set up an experiment.
///
/// Writes the experiment details to etcd and dumps key files to disk
/// (`key-{0..n}.json`, `keys.json`).
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    experiment: cli::ExperimentArgs,
    #[clap(flatten)]
    logs: cli::LogArgs,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let experiment = Experiment::from(args.experiment);
    let config = config::from_env().await?;
    write_to_store(&config, &experiment).await?;

    // let keys = experiment.get_keys();
    // for (idx, key) in keys.iter().enumerate() {
    //     let file = File::create(&format!("key-{}.json", idx))?;
    //     serde_json::to_writer(file, key)?;
    // }
    // {
    //     let file = File::create("keys.json")?;
    //     serde_json::to_writer(file, &keys)?;
    // }

    Ok(())
}
