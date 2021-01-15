fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .extern_path(
            ".spectrum_primitives.Integer",
            "::spectrum_primitives::proto::Integer",
        )
        .compile(&["proto/spectrum.proto"], &["proto"])?;
    Ok(())
}
