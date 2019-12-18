
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    spectrum_impl::run().await
}
