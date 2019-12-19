//! Spectrum implementation.
use std::time::Duration;
use tokio::timer::Timeout;
use std::rc::Rc;

pub mod client;
pub mod leader;
pub mod publisher;
pub mod server;

pub mod config;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Rc::new(config::InMemoryConfigStore::new());
    let _ = futures::join!(
        client::run(config_store.clone()),
        client::run(config_store.clone()),
        Timeout::new(server::run(config_store.clone()), Duration::from_secs(5)),
        publisher::run(),
        leader::run()
    );

    Ok(())
}
