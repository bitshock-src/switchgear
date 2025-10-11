
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let legacy_out = out_dir.join("legacy");
    std::fs::create_dir_all(&legacy_out)?;

    let lnd_proto_path = std::path::PathBuf::from("../proto/lnd_legacy");

    tonic_prost_build_0_14_2::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(&legacy_out)
        .compile_protos(&[lnd_proto_path.join("lightning.proto")], &[lnd_proto_path])?;

    let cln_proto_path = std::path::PathBuf::from("../proto/cln_legacy");

    tonic_prost_build_0_14_2::configure()
        .build_server(false)
        .build_client(true)
        .protoc_arg("--experimental_allow_proto3_optional")
        .out_dir(&legacy_out)
        .compile_protos(&[cln_proto_path.join("node.proto")], &[cln_proto_path])?;

    Ok(())
}