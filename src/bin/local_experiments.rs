use spectrum_impl::{experiment::Experiment, protocols::wrapper::ProtocolWrapper, run};

use clap::{crate_authors, crate_version, App, Arg};
use itertools::iproduct;
use serde::{Deserialize, Serialize};
use tokio::time::delay_for;

use std::io::{self, Write};
use std::iter::once;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
struct Record {
    groups: usize,
    group_size: u16,
    clients: u16,
    channels: usize,
    security_bytes: Option<u32>,
    msg_size: usize,
}

impl Record {
    fn new(
        groups: usize,
        group_size: u16,
        clients: u16,
        channels: usize,
        security_bytes: Option<u32>,
        msg_size: usize,
    ) -> Record {
        Record {
            groups,
            group_size,
            clients,
            channels,
            security_bytes,
            msg_size,
        }
    }
}

impl From<Record> for Experiment {
    fn from(record: Record) -> Experiment {
        let protocol = ProtocolWrapper::new(
            record.security_bytes,
            record.groups,
            record.channels,
            record.msg_size,
        );
        Experiment::new(protocol, record.group_size, record.clients)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // TODO(zjn): big hack. Write a dummy record to a string buffer, then remove the record.
    // When https://github.com/BurntSushi/rust-csv/issues/161 fixed, use that instead.
    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.serialize(Record::default())?;
    let headers = String::from_utf8((wtr).into_inner().unwrap()).unwrap();
    let headers = headers.split("\n").next().unwrap();
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
                            where security_bytes can be empty for the insecure protocol.
                            ",
                        headers
                    )
                    .as_str(),
                ),
        )
        .get_matches();

    let records: Box<dyn Iterator<Item = csv::Result<Record>>> = match matches.value_of("input") {
        None => {
            let groups = once(2usize);
            let group_sizes = once(2u16);
            let clients = (10u16..=50).step_by(10);
            let channels = once(1usize);
            let security_settings = vec![None, Some(40)].into_iter();
            let msg_sizes = once(1024);

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
                        Ok::<_, _>(Record::new(
                            groups, group_size, clients, channels, security, msg_size,
                        ))
                    },
                ),
            )
        }
        Some("-") => {
            let rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(io::stdin());
            Box::new(rdr.into_deserialize())
        }
        Some(path) => {
            let rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_path(path)?;
            Box::new(rdr.into_deserialize())
        }
    };

    let mut wtr = csv::Writer::from_writer(io::stdout());
    for record in records {
        let record: Record = record?;
        eprint!("Running: {:?}...", record);
        io::stderr().flush()?;

        let experiment = Experiment::from(record);
        match run(experiment).await {
            Ok(elapsed) => {
                // TODO: should include elapsed time!
                wtr.serialize(record)?;
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
