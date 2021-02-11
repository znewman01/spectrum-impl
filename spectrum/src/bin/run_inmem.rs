use clap::{crate_authors, crate_version, Clap};
use spectrum_impl::{cli, config, experiment::Experiment, run_in_process};
use tonic::transport::{Certificate, Identity};

/// Spectrum -- local testing client.
///
/// Run the Spectrum protocol locally.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    inner: cli::Args,
    #[clap(flatten)]
    tls_server: cli::TlsServerArgs,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    let cli = args.inner;
    cli.init_logs();

    let tls: Option<(Identity, Certificate)> = args.tls_server.into();
    let experiment = Experiment::from(cli);
    eprintln!("Running: {:?}...", experiment);
    let config = config::from_env().await?;
    let elapsed = run_in_process(experiment, config, tls).await?;
    eprintln!("Elapsed time: {:?}", elapsed);
    Ok(())
}
