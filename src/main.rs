use futures::executor::block_on;

fn main() {
    block_on(spectrum_impl::run());
}
