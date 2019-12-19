use spectrum_impl::{client, config};
use std::rc::Rc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Rc::new(config::InMemoryConfigStore::new());
    client::run(config_store).await
}
