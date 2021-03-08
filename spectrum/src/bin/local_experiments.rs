use spectrum_impl::{
    config, experiment::Experiment, protocols::wrapper::ProtocolWrapper, run_in_process,
};

use clap::{crate_authors, crate_version, Clap};
use itertools::iproduct;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use std::fs::File;
use std::io::{self, Write};
use std::iter::once;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
struct InputRecord {
    groups: usize,
    group_size: u16,
    clients: u128,
    channels: usize,
    security_bits: Option<u32>,
    msg_size: usize,
}

impl InputRecord {
    fn new(
        groups: usize,
        group_size: u16,
        clients: u128,
        channels: usize,
        security_bits: Option<u32>,
        msg_size: usize,
    ) -> InputRecord {
        InputRecord {
            groups,
            group_size,
            clients,
            channels,
            security_bits,
            msg_size,
        }
    }

    /// Get the headers when written to CSV as a comma-separated string.
    fn csv_headers() -> String {
        // TODO(zjn): big hack. Write a dummy record to a string buffer, then remove the record.
        // When https://github.com/BurntSushi/rust-csv/issues/161 fixed, use that instead.
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.serialize(Self::default()).unwrap();
        let headers = String::from_utf8((wtr).into_inner().unwrap()).unwrap();
        headers.split('\n').next().unwrap().to_string()
    }

    fn default_input_records() -> impl Iterator<Item = InputRecord> {
        let groups = once(2usize);
        let group_sizes = once(2u16);
        let clients = (10u128..=50).step_by(10);
        let channels = once(1usize);
        let security_settings = vec![None, Some(40)].into_iter();
        let msg_sizes = once(2 << 19); // 1 MB

        iproduct!(
            groups,
            group_sizes,
            clients,
            channels,
            security_settings,
            msg_sizes
        )
        .map(
            |(groups, group_size, clients, channels, security, msg_size)| {
                InputRecord::new(groups, group_size, clients, channels, security, msg_size)
            },
        )
    }
}

impl From<InputRecord> for Experiment {
    fn from(record: InputRecord) -> Experiment {
        let protocol = ProtocolWrapper::new(
            record.security_bits.is_some(),
            false, // TODO: allow multi key experiments
            record.groups,
            record.channels,
            record.msg_size,
        );
        let hammer = false;
        Experiment::new_sample_keys(protocol, record.group_size, record.clients, hammer)
    }
}

// TODO(zjn): use serde(flatten)
// https://github.com/BurntSushi/rust-csv/issues/98
#[derive(Serialize, Deserialize)]
struct OutputRecord {
    groups: usize,
    group_size: u16,
    clients: u128,
    channels: usize,
    security_bits: Option<u32>,
    msg_size: usize,
    elapsed_millis: u128,
}

impl OutputRecord {
    fn from_input_record(input: InputRecord, elapsed: Duration) -> Self {
        Self {
            groups: input.groups,
            group_size: input.group_size,
            clients: input.clients,
            channels: input.channels,
            security_bits: input.security_bits,
            msg_size: input.msg_size,
            elapsed_millis: elapsed.as_millis(),
        }
    }
}

fn get_input_help() -> String {
    format!(
        "\
        Columns are:\n\
        \t{}\n\
        where security_bits can be empty for the insecure protocol.\n\
        If omitted, runs a hard-coded set of parameters.\
        ",
        InputRecord::csv_headers()
    )
}

lazy_static! {
    static ref INPUT_STRING: &'static str = {
        // okay to leak this, it's small and needs to be created at runtime (and
        // will only be created once)
        Box::leak(get_input_help().into_boxed_str())
    };
}

/// Run the Spectrum protocol locally.
///
/// Collect data about local experiments (run in this process).
#[derive(Clap)]
#[clap(name = "local_experiments", version = crate_version!(), author = crate_authors!())]
struct Args {
    /// File (`-` for STDOUT) to write output CSV to.
    #[clap(long, short, default_value = "-")]
    output: String,

    /// File (`-` for STDIN) in .csv format.
    #[clap(
        long,
        short,
        long_help = &*INPUT_STRING,
    )]
    input: Option<String>,
}

type Error = Box<dyn std::error::Error + Sync + Send>;
type Result<T> = std::result::Result<T, Error>;

impl Args {
    fn csv_reader(&self) -> Option<csv::Reader<Box<dyn io::Read>>> {
        self.input.as_ref().map(|path| {
            let iordr: Box<dyn io::Read> = match path.as_str() {
                "-" => Box::new(io::stdin()),
                path => {
                    Box::new(File::create(path).expect("Could not create requested input file."))
                }
            };
            csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(iordr)
        })
    }

    fn csv_writer(&self) -> csv::Writer<Box<dyn io::Write>> {
        let iowtr: Box<dyn io::Write> = match self.output.as_str() {
            "-" => Box::new(io::stdout()),
            path => Box::new(File::create(path).expect("Could not create requested output file.")),
        };
        csv::Writer::from_writer(iowtr)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let records: Box<dyn Iterator<Item = csv::Result<InputRecord>>> = match args.csv_reader() {
        Some(reader) => Box::new(reader.into_deserialize()),
        None => Box::new(InputRecord::default_input_records().map(Ok)),
    };

    let mut wtr = args.csv_writer();
    for record in records {
        let record: InputRecord = record?;
        eprint!("Running: {:?}...", record);
        io::stderr().flush()?;

        let experiment = Experiment::from(record);
        let config = config::from_env().await?;
        match run_in_process(experiment, config, None).await {
            Ok(elapsed) => {
                let output = OutputRecord::from_input_record(record, elapsed);
                wtr.serialize(output)?;
                wtr.flush()?;
                eprintln!("done. elapsed time {:?}", elapsed);
            }
            Err(err) => {
                eprintln!("ERROR! {:?}", err);
            }
        };

        // allow time for wrap-up before next experiment
        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}
