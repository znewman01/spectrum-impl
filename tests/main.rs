extern crate spectrum_impl;

use simplelog::{LevelFilter, TermLogger, TerminalMode};

#[test]
fn test_pass() {
    // TODO: tokio::run in tokio 0.2.0+
    #![allow(unused_must_use)]
    TermLogger::init(
        LevelFilter::Trace,
        simplelog::ConfigBuilder::new()
            .add_filter_allow_str("spectrum_impl")
            .build(),
        TerminalMode::Stderr,
    )
    .unwrap();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(spectrum_impl::run());
}
