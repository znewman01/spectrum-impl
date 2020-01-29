fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/spectrum.proto")?;
    tonic_build::compile_protos("proto/health.proto")?;
    Ok(())
}
