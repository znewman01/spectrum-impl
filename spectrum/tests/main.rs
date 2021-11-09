extern crate spectrum;

use simplelog::{LevelFilter, TermLogger, TerminalMode};
use spectrum::{
    config, experiment::Experiment, protocols::wrapper::ProtocolWrapper, run_in_process,
};

#[tokio::test]
async fn test_pass() {
    TermLogger::init(
        LevelFilter::Trace,
        simplelog::ConfigBuilder::new()
            .add_filter_allow_str("spectrum")
            .build(),
        TerminalMode::Stderr,
    )
    .unwrap();

    let protocol = ProtocolWrapper::new(true, false, 2, 1, 100, false);
    let experiment = Experiment::new_sample_keys(protocol, 2, 3, false);

    let config = config::from_string("").await.unwrap();
    run_in_process(experiment, config, None).await.unwrap();
}
