use clap::{crate_authors, crate_version, Clap};
use spectrum_impl::{cli, config::EtcdRunner, experiment::Experiment, run_new_processes};

/// Spectrum -- local testing multi-process client.
///
/// Run the Spectrum protocol locally, with each party in a separate process.
///
/// This utility starts a local etcd instance for service discovery/registration.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    inner: cli::Args,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse().inner;
    args.init_logs();

    let experiment = Experiment::from(args);
    eprintln!("Running: {:?}...", experiment);

    let etcd_runner = EtcdRunner::create().await?;
    let config = etcd_runner.get_store().await?;

    run_new_processes(experiment, config, etcd_runner.get_env()).await?;

    Ok(())
}
