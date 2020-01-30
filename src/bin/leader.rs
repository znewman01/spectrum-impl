use spectrum_impl::{config, leader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::from_env()?;
    leader::run(&config).await
}
