use spectrum_impl::{
    config, leader,
    services::discovery::{Group, Service},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let service = Service::Leader { group: Group(1) };
    leader::run(config, service).await
}
