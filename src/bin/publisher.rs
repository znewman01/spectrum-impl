use spectrum_impl::publisher;
use futures::executor::block_on;

fn main() {
    block_on(publisher::run());
}
