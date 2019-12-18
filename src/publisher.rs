use futures::executor::block_on;

pub async fn run() {
    println!("Hello, world from the publisher!");
}

#[allow(dead_code)]
fn main() {
    block_on(run());
}
