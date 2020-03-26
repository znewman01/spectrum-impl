use spectrum_impl::{experiment::Experiment, protocols::wrapper::ProtocolWrapper, run};

use clap::{crate_authors, crate_version, App, Arg};
use itertools::iproduct;
use serde::{Deserialize, Serialize};
use tokio::time::delay_for;

use std::fs::File;
use std::io::{self, Write};
use std::iter::once;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
struct InputRecord {
    groups: usize,
    group_size: u16,
    clients: u16,
    channels: usize,
    security_bits: Option<u32>,
    msg_size: usize,
}

impl InputRecord {
    fn new(
        groups: usize,
        group_size: u16,
        clients: u16,
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
}

impl From<InputRecord> for Experiment {
    fn from(record: InputRecord) -> Experiment {
        let protocol = ProtocolWrapper::new(
            record.security_bits,
            record.groups,
            record.channels,
            record.msg_size,
        );
        Experiment::new(protocol, record.group_size, record.clients)
    }
}

// TODO(zjn): use serde(flatten)
// https://github.com/BurntSushi/rust-csv/issues/98
#[derive(Serialize, Deserialize)]
struct OutputRecord {
    groups: usize,
    group_size: u16,
    clients: u16,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // TODO(zjn): big hack. Write a dummy record to a string buffer, then remove the record.
    // When https://github.com/BurntSushi/rust-csv/issues/161 fixed, use that instead.
    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.serialize(InputRecord::default())?;
    let headers = String::from_utf8((wtr).into_inner().unwrap()).unwrap();
    let headers = headers.split('\n').next().unwrap();
    let matches = App::new("Spectrum -- run local protocol experiments")
        .version(crate_version!())
        .about("Collect data about local experiments.")
        .author(crate_authors!())
        .arg(
            Arg::with_name("input")
                .long("input")
                .short("i")
                .takes_value(true)
                .help("File (`-` for STDIN) in .csv format.")
                .long_help(
                    format!(
                        "File (`-` for STDIN) in .csv format. \
                         Columns are: \n\
                         \t{}\n\
                         where security_bits can be empty for the insecure protocol.\n\n\
                         If omitted, runs a quick hard-coded set of parameters.",
                        headers
                    )
                    .as_str(),
                ),
        )
        .arg(
            Arg::with_name("output")
                .long("output")
                .short("o")
                .takes_value(true)
                .default_value("-")
                .help("File (`-` for STDOUT) to write output CSV to."),
        )
        .get_matches();

    let records: Box<dyn Iterator<Item = csv::Result<InputRecord>>> =
        match matches.value_of("input") {
            Some(path) => {
                let iordr: Box<dyn io::Read> = if path == "-" {
                    Box::new(io::stdin())
                } else {
                    Box::new(File::create(path)?)
                };
                let rdr = csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(iordr);
                Box::new(rdr.into_deserialize())
            }
            None => {
                let groups = once(2usize);
                let group_sizes = once(2u16);
                let clients = (10u16..=50).step_by(10);
                let channels = once(1usize);
                let security_settings = vec![None, Some(40)].into_iter();
                let msg_sizes = once(2 << 19); // 1 MB

                Box::new(
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
                            Ok::<_, _>(InputRecord::new(
                                groups, group_size, clients, channels, security, msg_size,
                            ))
                        },
                    ),
                )
            }
        };

    let iowtr: Box<dyn io::Write> = match matches.value_of("output").expect("has default") {
        "-" => Box::new(io::stdout()),
        path => Box::new(File::create(path)?),
    };
    let mut wtr = csv::Writer::from_writer(iowtr);

    for record in records {
        let record: InputRecord = record?;
        eprint!("Running: {:?}...", record);
        io::stderr().flush()?;

        let experiment = Experiment::from(record);
        match run(experiment).await {
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
        delay_for(Duration::from_millis(100)).await;
    }

    Ok(())
}
