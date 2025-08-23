fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;

    let path = Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../protobuf/bigworlds.proto");
    tonic_build::compile_protos(path)?;
    Ok(())
}
