use anyhow::bail;
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendSparse,
};
use switchgear_service::components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_service::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcClientAuthPath, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_testing::credentials::{get_backends, RegTestLnNode, RegTestLnNodeAddress};
use url::Url;

#[path = "../common/mod.rs"]
pub mod common;

mod cln_tonic;
mod lnd_tonic;

pub fn try_create_cln_backend() -> anyhow::Result<Option<DiscoveryBackend>> {
    let backends = get_backends()?;

    if backends.is_empty() {
        return Ok(None);
    }

    let cln_node = backends
        .into_iter()
        .filter_map(|b| match b {
            RegTestLnNode::Cln(cln) => Some(cln),
            _ => None,
        })
        .next()
        .ok_or_else(|| anyhow::anyhow!("no cln nodes available"))?;

    let url = match cln_node.address {
        RegTestLnNodeAddress::Inet(addr) => Url::parse(&format!("https://{addr}"))?,
        RegTestLnNodeAddress::Path(_) => bail!("socket path not supported"),
    };

    let address = DiscoveryBackendAddress::PublicKey(cln_node.public_key);

    let implementation =
        DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
            url,
            auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                ca_cert_path: cln_node.ca_cert_path,
                client_cert_path: cln_node.client_cert_path,
                client_key_path: cln_node.client_key_path,
            }),
            domain: Some(cln_node.sni),
        });

    let backend = DiscoveryBackend {
        address,
        backend: DiscoveryBackendSparse {
            partitions: ["default".to_string()].into(),
            weight: 1,
            implementation,
            enabled: true,
        },
    };

    Ok(Some(backend))
}

pub fn try_create_lnd_backend() -> anyhow::Result<Option<DiscoveryBackend>> {
    let backends = get_backends()?;

    if backends.is_empty() {
        return Ok(None);
    }

    let lnd_node = backends
        .into_iter()
        .filter_map(|b| match b {
            RegTestLnNode::Lnd(lnd) => Some(lnd),
            _ => None,
        })
        .next()
        .ok_or_else(|| anyhow::anyhow!("no lnd nodes available"))?;

    let address = DiscoveryBackendAddress::PublicKey(lnd_node.public_key);

    let url = match lnd_node.address {
        RegTestLnNodeAddress::Inet(addr) => Url::parse(&format!("https://{addr}"))?,
        RegTestLnNodeAddress::Path(_) => bail!("socket path not supported"),
    };

    let implementation =
        DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
            url,
            auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                tls_cert_path: lnd_node.tls_cert_path,
                macaroon_path: lnd_node.macaroon_path,
            }),
            amp_invoice: false,
            domain: None,
        });

    let backend = DiscoveryBackend {
        address: address.clone(),
        backend: DiscoveryBackendSparse {
            partitions: ["default".to_string()].into(),
            weight: 1,
            implementation,
            enabled: true,
        },
    };

    Ok(Some(backend))
}
