fn main() -> Result<(), Box<dyn std::error::Error>> {

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["proto/lnd/lightning.proto"], &[])?;

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["proto/cln/node.proto"], &["proto/cln"])?;

    Ok(())
}