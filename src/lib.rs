//! Spectrum implementation.

pub mod client;
pub mod leader;
pub mod publisher;
pub mod server;

pub async fn run() {
    futures::join!(
        client::run(),
        client::run(),
        server::run(),
        publisher::run(),
        leader::run()
    );
}
