fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc_server")]
    {
        use std::path::Path;

        let path = Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../proto/worlds.proto");
        tonic_build::compile_protos(path)?;
    }
    Ok(())
}
