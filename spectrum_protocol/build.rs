fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "proto")]
    prost_build::compile_protos(&["proto/protocol.proto"], &["proto"])?;
    Ok(())
}
