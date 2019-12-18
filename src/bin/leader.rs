use spectrum_impl::leader;
use futures::executor::block_on;

fn main() {
    block_on(leader::run());
}
