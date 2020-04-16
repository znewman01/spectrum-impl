extern crate spectrum_impl;

use simplelog::{LevelFilter, TermLogger, TerminalMode};
use spectrum_impl::{
    config, experiment::Experiment, protocols::wrapper::ProtocolWrapper, run_in_process,
};

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

    let protocol = ProtocolWrapper::new(Some(40), false, 2, 3, 1024);
    let experiment = Experiment::new(protocol, 2, 6);

    let config = config::from_string("").await.unwrap();
    run_in_process(experiment, config).await.unwrap();
}
