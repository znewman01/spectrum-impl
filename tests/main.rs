extern crate spectrum_impl;

use simplelog::{LevelFilter, TermLogger, TerminalMode};

#[tokio::test]
async fn test_pass() {
    TermLogger::init(
        LevelFilter::Trace,
        simplelog::ConfigBuilder::new()
            .add_filter_allow_str("spectrum_impl")
            .build(),
        TerminalMode::Stderr,
    )
    .unwrap();
    spectrum_impl::run(2, 2, 10, 3).await.unwrap();
}
