extern crate spectrum_impl;

use futures::executor::block_on;

#[test]
fn test_pass() {
    block_on(spectrum_impl::run());
}
