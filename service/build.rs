
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let ln_out = out_dir.join("ln");
    std::fs::create_dir_all(&ln_out)?;

    let lnd_proto_path = std::path::PathBuf::from("../proto/lnd");

    tonic_prost_build_0_14_2::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(&ln_out)
        .compile_protos(&[lnd_proto_path.join("lightning.proto")], &[lnd_proto_path])?;

    let cln_proto_path = std::path::PathBuf::from("../proto/cln");

    tonic_prost_build_0_14_2::configure()
        .build_server(false)
        .build_client(true)
        .out_dir(&ln_out)
        .compile_protos(&[cln_proto_path.join("node.proto")], &[cln_proto_path])?;

    Ok(())
}