use spectrum_impl::experiment::config;

use clap::{crate_authors, crate_version, Clap};
use failure::Error;
use serde::ser::{SerializeSeq, Serializer};

use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::time::Duration;

type Result<T> = std::result::Result<T, Error>;

/// Spectrum -- driver for cloud-based experiments.
///
/// Runs the whole protocol on AWS defined-duration spot instances.
///
///
/// Uses typical AWS `$AWS_ACCESS_KEY_ID`, `$AWS_SECRET_ACCESS_KEY` environment
/// variables for authentication.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    /// Path to a JSON file containing the experiments to run.
    ///
    /// The file should contain a list of input records; an example is
    /// distributed with the source of this utility (`experiments.json`).
    #[clap()]
    experiments_file: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // TODO: init `slog` for Tsunami logging.

    let experiments: Vec<config::Experiment>;
    {
        let file = File::open(args.experiments_file)?;
        experiments = serde_json::from_reader(file)?;
    }

    let _machine_types: HashSet<String> = experiments
        .iter()
        .flat_map(|e| e.instance_types())
        .collect();
    // TODO: compile on target machines of each machine type

    // Stream the results to STDOUT.
    let mut serializer = serde_json::Serializer::new(io::stdout());
    let mut seq = serializer.serialize_seq(None)?;

    let experiment_sets = config::Experiment::by_environment(experiments);
    for (_environment, experiments) in experiment_sets {
        // Performance optimizations:
        // - make our own AMI
        // - many rounds

        // TODO: bring up environment

        for experiment in experiments {
            // TODO: run the experiment
            // - set SPECTRUM_CONFIG_SERVER=etcd://<private IP of publisher>
            // - dump experiment to store! (will require messing with security groups)
            // - trigger build via publisher!

            let time = Duration::from_millis(1100);

            let result = config::Result::new(experiment, time);
            seq.serialize_element(&result)?;
        }
    }
    seq.end()?;

    Ok(())
}
