use futures::executor::block_on;
use spectrum_impl::publisher;

fn main() {
    block_on(publisher::run());
}
