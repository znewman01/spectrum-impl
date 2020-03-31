use clap::{crate_authors, crate_version, Clap};
use spectrum_impl::{cli, config, experiment::Experiment, run_in_process};

/// Spectrum -- local testing client.
///
/// Run the Spectrum protocol locally.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    common: cli::Args,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.common.init_logs();

    let experiment = Experiment::from(args.common);
    eprintln!("Running: {:?}...", experiment);
    let config = config::from_env().await?;
    let elapsed = run_in_process(experiment, config).await?;
    eprintln!("Elapsed time: {:?}", elapsed);
    Ok(())
}
