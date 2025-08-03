use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    tonic_prost_build::configure()
        .file_descriptor_set_path(PathBuf::from(env::var("OUT_DIR").unwrap()).join("mediator_descriptor.bin"))
        .compile_protos(&["./proto/mediator.proto"], &["./proto"])?;
    Ok(())
}