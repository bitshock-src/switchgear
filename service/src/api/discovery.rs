use crate::api::service::HasServiceErrorSource;
use crate::components::pool::cln::grpc::config::ClnGrpcDiscoveryBackendImplementation;
use crate::components::pool::lnd::grpc::config::LndGrpcDiscoveryBackendImplementation;
use async_trait::async_trait;
use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::io;

#[async_trait]
pub trait DiscoveryBackendStore {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error>;

    async fn get_all(&self, etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error>;

    async fn post(&self, backend: DiscoveryBackend) -> Result<Option<PublicKey>, Self::Error>;

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error>;

    async fn patch(&self, backend: DiscoveryBackendPatch) -> Result<bool, Self::Error>;

    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error>;
}

#[async_trait]
pub trait HttpDiscoveryBackendClient: DiscoveryBackendStore {
    async fn health(&self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackends {
    pub etag: u64,
    pub backends: Option<Vec<DiscoveryBackend>>,
}

impl DiscoveryBackends {
    pub fn etag_from_str(etag: &str) -> io::Result<u64> {
        let etag = hex::decode(etag).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let etag: [u8; 8] = etag.try_into().map_err(|etag: Vec<u8>| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid etag size: {} bytes", etag.len()),
            )
        })?;
        Ok(u64::from_be_bytes(etag))
    }

    pub fn etag_string(&self) -> String {
        hex::encode(self.etag.to_be_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackend {
    pub public_key: PublicKey,
    #[serde(flatten)]
    pub backend: DiscoveryBackendSparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackendSparse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub partitions: BTreeSet<String>,
    pub weight: usize,
    pub enabled: bool,
    pub implementation: DiscoveryBackendImplementation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackendPatch {
    pub public_key: PublicKey,
    #[serde(flatten)]
    pub backend: DiscoveryBackendPatchSparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackendPatchSparse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partitions: Option<BTreeSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum DiscoveryBackendImplementation {
    ClnGrpc(ClnGrpcDiscoveryBackendImplementation),
    LndGrpc(LndGrpcDiscoveryBackendImplementation),
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::components::pool::lnd::grpc::config::{LndGrpcClientAuth, LndGrpcClientAuthPath};
    use secp256k1::{Secp256k1, SecretKey};

    #[test]
    fn serialize_when_discovery_backend_then_returns_json_with_flattened_fields() {
        let private_key = SecretKey::from_byte_array([
            0xe1, 0x26, 0xf6, 0x8f, 0x7e, 0xaf, 0xcc, 0x8b, 0x74, 0xf5, 0x4d, 0x26, 0x9f, 0xe2,
            0x06, 0xbe, 0x71, 0x50, 0x00, 0xf9, 0x4d, 0xac, 0x06, 0x7d, 0x1c, 0x04, 0xa8, 0xca,
            0x3b, 0x2d, 0xb7, 0x34,
        ])
        .unwrap();
        let public_key = private_key.public_key(&Secp256k1::new());

        let backend = DiscoveryBackend {
            public_key,
            backend: DiscoveryBackendSparse {
                name: None,
                partitions: ["default".to_string()].into(),
                weight: 0,
                enabled: true,
                implementation: DiscoveryBackendImplementation::LndGrpc(
                    LndGrpcDiscoveryBackendImplementation {
                        url: "https://localhost:9736".parse().unwrap(),
                        domain: None,
                        auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                            tls_cert_path: None,
                            macaroon_path: "/path/to/macaroon_path".into(),
                        }),
                        amp_invoice: false,
                    },
                ),
            },
        };

        let backends = serde_json::to_string(&backend).unwrap();
        let backends_expected = r#"{"publicKey":"03e7156ae33b0a208d0744199163177e909e80176e55d97a2f221ede0f934dd9ad","partitions":["default"],"weight":0,"enabled":true,"implementation":{"type":"lndGrpc","url":"https://localhost:9736/","domain":null,"auth":{"type":"path","tlsCertPath":null,"macaroonPath":"/path/to/macaroon_path"},"ampInvoice":false}}"#;
        assert_eq!(backends, backends_expected);
    }

    #[test]
    fn deserialize_when_valid_json_then_creates_discovery_backend_with_flattened_fields() {
        let private_key = SecretKey::from_byte_array([
            0xe1, 0x26, 0xf6, 0x8f, 0x7e, 0xaf, 0xcc, 0x8b, 0x74, 0xf5, 0x4d, 0x26, 0x9f, 0xe2,
            0x06, 0xbe, 0x71, 0x50, 0x00, 0xf9, 0x4d, 0xac, 0x06, 0x7d, 0x1c, 0x04, 0xa8, 0xca,
            0x3b, 0x2d, 0xb7, 0x34,
        ])
        .unwrap();
        let public_key = private_key.public_key(&Secp256k1::new());

        let backend_expected = DiscoveryBackend {
            public_key,
            backend: DiscoveryBackendSparse {
                name: None,
                partitions: ["default".to_string()].into(),
                weight: 0,
                enabled: true,
                implementation: DiscoveryBackendImplementation::LndGrpc(
                    LndGrpcDiscoveryBackendImplementation {
                        url: "https://localhost:9736".parse().unwrap(),
                        domain: None,
                        auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                            tls_cert_path: None,
                            macaroon_path: "/path/to/macaroon_path".into(),
                        }),
                        amp_invoice: false,
                    },
                ),
            },
        };

        let backend = r#"{"publicKey":"03e7156ae33b0a208d0744199163177e909e80176e55d97a2f221ede0f934dd9ad","partitions":["default"],"weight":0,"enabled":true,"implementation":{"type":"lndGrpc","url":"https://localhost:9736/","domain":null,"auth":{"type":"path","tlsCertPath":null,"macaroonPath":"/path/to/macaroon_path"},"ampInvoice":false}}"#;
        let backend: DiscoveryBackend = serde_json::from_str(backend).unwrap();

        assert_eq!(backend_expected, backend);
    }
}
