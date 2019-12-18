extern crate spectrum_impl;

#[test]
fn test_pass() {
    // TODO: tokio::run in tokio 0.2.0+
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(spectrum_impl::run());
}
