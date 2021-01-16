fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "proto")]
    tonic_build::compile_protos("proto/integer.proto")?;
    Ok(())
}
