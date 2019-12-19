use spectrum_impl::{config, server};
use std::rc::Rc;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Rc::new(config::InMemoryConfigStore::new());
    server::run(config_store).await
}
