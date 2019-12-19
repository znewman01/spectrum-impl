use futures::executor::block_on;
use spectrum_impl::leader;

fn main() {
    block_on(leader::run());
}
