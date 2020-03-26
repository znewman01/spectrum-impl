use spectrum_impl::{experiment::Experiment, protocols::wrapper::ProtocolWrapper, run};

use itertools::iproduct;
use serde::{Deserialize, Serialize};
use tokio::time::delay_for;

use std::io::{self, Write};
use std::iter::once;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
struct ExperimentRecord {
    groups: usize,
    group_size: u16,
    clients: u16,
    channels: usize,
    security_bytes: Option<u32>,
    msg_size: usize,
}

impl ExperimentRecord {
    fn new(
        groups: usize,
        group_size: u16,
        clients: u16,
        channels: usize,
        security_bytes: Option<u32>,
        msg_size: usize,
    ) -> ExperimentRecord {
        ExperimentRecord {
            groups,
            group_size,
            clients,
            channels,
            security_bytes,
            msg_size,
        }
    }
}

impl From<ExperimentRecord> for Experiment {
    fn from(record: ExperimentRecord) -> Experiment {
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
    let groups = once(2usize);
    let group_sizes = once(2u16);
    let clients = (10u16..=50).step_by(10);
    let channels = once(1usize);
    let security_settings = vec![None, Some(40)].into_iter();
    let msg_sizes = once(1024);

    let records = iproduct!(
        groups,
        group_sizes,
        clients,
        channels,
        security_settings,
        msg_sizes
    )
    .map(
        |(groups, group_size, clients, channels, security, msg_size)| {
            ExperimentRecord::new(groups, group_size, clients, channels, security, msg_size)
        },
    );

    let mut wtr = csv::Writer::from_writer(io::stdout());
    for record in records {
        eprint!("Running: {:?}...", record);
        io::stderr().flush()?;

        let experiment = Experiment::from(record);
        match run(experiment).await {
            Ok(elapsed) => {
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
