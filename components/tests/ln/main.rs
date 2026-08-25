use std::collections::BTreeMap;
use switchgear_components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_components::secrets::{SecretStoreConfig, SecretStoreSecretConfig};
use switchgear_testing::credentials::lightning::LnCredentials;
use url::Url;

#[path = "../common/mod.rs"]
pub mod common;

mod cln;
mod lnd;

pub fn try_create_cln_backend_implementation(
    credentials: &LnCredentials,
) -> anyhow::Result<ClnGrpcDiscoveryBackendImplementation> {
    let cln_node = credentials.get_backends()?.cln.clone();
    let url = Url::parse(&format!("https://{}", cln_node.address))?;

    Ok(ClnGrpcDiscoveryBackendImplementation {
        url,
        domain: None,
        auth: ClnGrpcClientAuth {
            ca_cert_path: cln_node.ca_cert_path.into(),
            client_cert_secret: "CLIENT_CERT".to_owned(),
            client_key_secret: "CLIENT_KEY".to_owned(),
            secrets: SecretStoreConfig {
                ttl: 60.0,
                secrets: BTreeMap::from([
                    (
                        "CLIENT_CERT".to_owned(),
                        SecretStoreSecretConfig {
                            path: cln_node.client_cert_path,
                        },
                    ),
                    (
                        "CLIENT_KEY".to_owned(),
                        SecretStoreSecretConfig {
                            path: cln_node.client_key_path,
                        },
                    ),
                ]),
            },
        },
    })
}

pub fn try_create_lnd_backend_implementation(
    credentials: &LnCredentials,
) -> anyhow::Result<LndGrpcDiscoveryBackendImplementation> {
    let lnd_node = credentials.get_backends()?.lnd.clone();
    let url = Url::parse(&format!("https://{}", lnd_node.address))?;

    Ok(LndGrpcDiscoveryBackendImplementation {
        url,
        domain: None,
        auth: LndGrpcClientAuth {
            tls_cert_path: lnd_node.tls_cert_path.into(),
            macaroon_secret: "MACAROON".to_owned(),
            secrets: SecretStoreConfig {
                ttl: 60.0,
                secrets: BTreeMap::from([(
                    "MACAROON".to_owned(),
                    SecretStoreSecretConfig {
                        path: lnd_node.macaroon_path,
                    },
                )]),
            },
        },
        amp_invoice: false,
    })
}
