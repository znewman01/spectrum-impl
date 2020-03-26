use itertools::iproduct;
use spectrum_impl::{experiment::Experiment, protocols::wrapper::ProtocolWrapper, run};
use std::io::{self, Write};
use std::iter::once;
use std::time::Duration;
use tokio::time::delay_for;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let groups = once(2usize);
    let group_sizes = once(2u16);
    let clients = (10u16..=100).step_by(10);
    let channels = once(1usize);
    let security_settings = vec![None, Some(40)].into_iter();
    let msg_sizes = once(1024);

    for (groups, group_size, clients, channels, security, msg_size) in iproduct!(
        groups,
        group_sizes,
        clients,
        channels,
        security_settings,
        msg_sizes
    ) {
        let protocol = ProtocolWrapper::new(security, groups, channels, msg_size);
        let experiment = Experiment::new(protocol, group_size, clients);

        eprint!("Running: {:?}...", experiment);
        io::stderr().flush()?;

        match run(experiment).await {
            Ok(elapsed) => {
                // TODO: emit structured output
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
