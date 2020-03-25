use itertools::iproduct;
use std::iter::once;
use std::time::Duration;
use tokio::time::delay_for;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let groups = once(2u16);
    let group_sizes = once(2u16);
    let clients = (100u16..=200).step_by(10);
    let channels = once(1usize);
    for (groups, group_size, clients, channels) in iproduct!(groups, group_sizes, clients, channels)
    {
        let result = spectrum_impl::run(groups, group_size, clients, channels).await;
        match result {
            Ok(elapsed) => {
                println!(
                    "elapsed time (groups={}, group_size={}, clients={}, channels={}): {:?}",
                    groups, group_size, clients, channels, elapsed
                );
            }
            Err(err) => {
                println!(
                    "ERROR! (groups={}, group_size={}, clients={}, channels={}): {:?}",
                    groups, group_size, clients, channels, err
                );
            }
        };
        delay_for(Duration::from_millis(100)).await; // allow time for wrap-up
    }

    Ok(())
}
