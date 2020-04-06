use spectrum_impl::experiment::{compile, config};

use clap::{crate_authors, crate_version, Clap};
use failure::Error;
use serde::ser::{SerializeSeq, Serializer};
use std::path::PathBuf;

use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::time::Duration;

const BASE_AMI: &str = "ami-0fc20dd1da406780b";

type Result<T> = std::result::Result<T, Error>;

/// Spectrum -- driver for cloud-based experiments.
///
/// Runs the whole protocol on AWS defined-duration spot instances.
///
/// Uses typical AWS `$AWS_ACCESS_KEY_ID`, `$AWS_SECRET_ACCESS_KEY` environment
/// variables for authentication.
#[derive(Clap, Clone)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    /// Path to a JSON file containing the experiments to run.
    ///
    /// The file should contain a list of input records; an example is
    /// distributed with the source of this utility (`experiments.json`).
    #[clap()]
    experiments_file: String,

    /// Path to a directory where compiled binaries should be stored.
    #[clap(long)]
    binary_dir: PathBuf,

    /// Whether to compile the binaries in debug mode.
    ///
    /// This is much faster but the performance of the ultimate binaries workse.
    #[clap(long)]
    debug: bool,
}

impl Args {
    fn profile(&self) -> compile::Profile {
        if self.debug {
            compile::Profile::Debug
        } else {
            compile::Profile::Release
        }
    }
}

fn init_logger() -> slog::Logger {
    use slog::o;
    use slog::Drain;
    use std::sync::Mutex;

    let decorator = slog_term::TermDecorator::new().build();
    let drain = Mutex::new(slog_term::FullFormat::new(decorator).build()).fuse();
    slog::Logger::root(drain, o!())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log = init_logger();

    let experiments: Vec<config::Experiment>;
    {
        let file = File::open(args.experiments_file.clone())?;
        experiments = serde_json::from_reader(file)?;
    }

    let machine_types: HashSet<String> = experiments
        .iter()
        .flat_map(|e| e.instance_types())
        .collect();
    let src_dir = PathBuf::from("/home/zjn/git/spectrum-impl/");
    compile::compile(
        &log,
        args.binary_dir.clone(),
        src_dir,
        machine_types.into_iter().collect(),
        args.profile(),
        BASE_AMI.to_string(),
    )
    .await?;
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
