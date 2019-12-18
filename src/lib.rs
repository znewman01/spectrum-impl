//! Spectrum implementation.
use std::time::Duration;
use tokio::timer::Timeout;

pub mod client;
pub mod leader;
pub mod publisher;
pub mod server;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _ = futures::join!(
        client::run(),
        client::run(),
        Timeout::new(server::run(), Duration::from_secs(5)),
        publisher::run(),
        leader::run()
    );

    Ok(())
}
