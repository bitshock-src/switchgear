


fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    let lnd_proto_path = std::path::PathBuf::from("../proto/lnd");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[lnd_proto_path.join("lightning.proto")], &[lnd_proto_path])?;

    let cln_proto_path = std::path::PathBuf::from("../proto/cln");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[cln_proto_path.join("node.proto")], &[cln_proto_path])?;

    Ok(())
}