use spectrum_impl::{
    config, leader,
    services::{Group, LeaderInfo},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let info = LeaderInfo::new(Group::new(1));
    leader::run(config, info).await
}
