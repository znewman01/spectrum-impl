use simplelog::{CombinedLogger, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Trace,
            simplelog::ConfigBuilder::new()
                .add_filter_allow_str("spectrum_impl")
                .build(),
            TerminalMode::Stderr,
        )
        .unwrap(),
        WriteLogger::new(
            LevelFilter::Trace,
            simplelog::Config::default(),
            File::create("spectrum.log").unwrap(),
        ),
    ])
    .unwrap();
    spectrum_impl::run().await
}
