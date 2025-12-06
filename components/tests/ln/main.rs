use switchgear_components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcClientAuthPath, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_testing::credentials::lightning::LnCredentials;
use url::Url;

#[path = "../common/mod.rs"]
pub mod common;

mod cln;
mod lnd;

pub fn try_create_cln_backend_implementation(
    credentials: &LnCredentials,
) -> anyhow::Result<ClnGrpcDiscoveryBackendImplementation> {
    let backends = credentials.get_backends()?;

    let cln_node = backends.cln.clone();

    let url = Url::parse(&format!("https://{}", cln_node.address))?;

    let implementation = ClnGrpcDiscoveryBackendImplementation {
        url,
        auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
            ca_cert_path: cln_node.ca_cert_path.into(),
            client_cert_path: cln_node.client_cert_path,
            client_key_path: cln_node.client_key_path,
        }),
        domain: None,
    };

    Ok(implementation)
}

pub fn try_create_lnd_backend_implementation(
    credentials: &LnCredentials,
) -> anyhow::Result<LndGrpcDiscoveryBackendImplementation> {
    let backends = credentials.get_backends()?;

    let lnd_node = backends.lnd.clone();

    let url = Url::parse(&format!("https://{}", lnd_node.address))?;

    let implementation = LndGrpcDiscoveryBackendImplementation {
        url,
        auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
            tls_cert_path: lnd_node.tls_cert_path.into(),
            macaroon_path: lnd_node.macaroon_path,
        }),
        amp_invoice: false,
        domain: None,
    };

    Ok(implementation)
}
