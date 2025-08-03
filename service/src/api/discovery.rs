use crate::api::service::HasServiceErrorSource;
use crate::components::pool::cln::grpc::config::ClnGrpcDiscoveryBackendImplementation;
use crate::components::pool::lnd::grpc::config::LndGrpcDiscoveryBackendImplementation;
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
use std::str::FromStr;
use url::Url;

#[async_trait]
pub trait DiscoveryBackendStore {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn get(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error>;

    async fn get_all(&self, partition: &str) -> Result<Vec<DiscoveryBackend>, Self::Error>;

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error>;

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error>;

    async fn delete(
        &self,
        partition: &str,
        addr: &DiscoveryBackendAddress,
    ) -> Result<bool, Self::Error>;
}

#[async_trait]
pub trait HttpDiscoveryBackendClient: DiscoveryBackendStore {
    async fn health(&self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackend {
    pub partition: String,
    pub address: DiscoveryBackendAddress,
    #[serde(flatten)]
    pub backend: DiscoveryBackendSparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackendRest {
    pub location: String,
    #[serde(flatten)]
    pub backend: DiscoveryBackend,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryBackendAddress {
    PublicKey(PublicKey),
    Url(Url),
}

impl Display for DiscoveryBackendAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryBackendAddress::PublicKey(addr) => write!(f, "{addr}"),
            DiscoveryBackendAddress::Url(addr) => write!(f, "{addr}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryBackendSparse {
    pub weight: usize,
    pub enabled: bool,
    pub implementation: DiscoveryBackendImplementation,
}

impl DiscoveryBackendAddress {
    pub fn encoded(&self) -> String {
        match self {
            DiscoveryBackendAddress::PublicKey(k) => format!("pk/{k}"),
            DiscoveryBackendAddress::Url(u) => {
                format!("url/{}", URL_SAFE_NO_PAD.encode(u.to_string().as_bytes()))
            }
        }
    }
}

impl FromStr for DiscoveryBackendAddress {
    type Err = io::Error;
    fn from_str(s: &str) -> io::Result<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid format: expected 'variant/base64'",
            ));
        }

        let variant = parts[0];
        let encoded_addr = parts[1];

        match variant {
            "pk" => {
                let pk = encoded_addr
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Self::PublicKey(pk))
            }
            "url" => {
                let url = URL_SAFE_NO_PAD
                    .decode(encoded_addr)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                let url = str::from_utf8(&url)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                let url =
                    Url::parse(url).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Self::Url(url))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown variant '{variant}'"),
            )),
        }
    }
}

impl<S> TryFrom<(S, S)> for DiscoveryBackendAddress
where
    S: AsRef<str> + Display,
{
    type Error = io::Error;

    fn try_from(value: (S, S)) -> Result<Self, Self::Error> {
        let formatted_str = format!("{}/{}", value.0, value.1);
        Self::from_str(&formatted_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum DiscoveryBackendImplementation {
    ClnGrpc(ClnGrpcDiscoveryBackendImplementation),
    LndGrpc(LndGrpcDiscoveryBackendImplementation),
    RemoteHttp,
}

#[cfg(test)]
mod test {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};

    #[test]
    fn serialize_when_discovery_backend_then_returns_json_with_flattened_fields() {
        let key = SecretKey::from_slice(
            &[
                0xe1, 0x26, 0xf6, 0x8f, 0x7e, 0xaf, 0xcc, 0x8b, 0x74, 0xf5, 0x4d, 0x26, 0x9f, 0xe2,
                0x06, 0xbe, 0x71, 0x50, 0x00, 0xf9, 0x4d, 0xac, 0x06, 0x7d, 0x1c, 0x04, 0xa8, 0xca,
                0x3b, 0x2d, 0xb7, 0x34,
            ][..],
        )
        .unwrap();
        let key = key.public_key(&Secp256k1::new());

        let backend = DiscoveryBackendSparse {
            weight: 0,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        };
        let address = DiscoveryBackendAddress::PublicKey(key);
        let backend1 = DiscoveryBackend {
            partition: "default".to_string(),
            address,
            backend,
        };

        let backend = DiscoveryBackendSparse {
            weight: 0,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        };
        let address = DiscoveryBackendAddress::Url(Url::parse("http://example.com/").unwrap());
        let backend2 = DiscoveryBackend {
            partition: "default".to_string(),
            address,
            backend,
        };

        let backends = vec![backend1, backend2];

        let backends = serde_json::to_string(&backends).unwrap();
        let backends_expected = r#"[{"partition":"default","address":{"publicKey":"03e7156ae33b0a208d0744199163177e909e80176e55d97a2f221ede0f934dd9ad"},"weight":0,"enabled":true,"implementation":{"type":"remoteHttp"}},{"partition":"default","address":{"url":"http://example.com/"},"weight":0,"enabled":true,"implementation":{"type":"remoteHttp"}}]"#;
        assert_eq!(backends, backends_expected);
    }

    #[test]
    fn deserialize_when_valid_json_then_creates_discovery_backend_with_flattened_fields() {
        let key = SecretKey::from_slice(
            &[
                0xe1, 0x26, 0xf6, 0x8f, 0x7e, 0xaf, 0xcc, 0x8b, 0x74, 0xf5, 0x4d, 0x26, 0x9f, 0xe2,
                0x06, 0xbe, 0x71, 0x50, 0x00, 0xf9, 0x4d, 0xac, 0x06, 0x7d, 0x1c, 0x04, 0xa8, 0xca,
                0x3b, 0x2d, 0xb7, 0x34,
            ][..],
        )
        .unwrap();
        let key = key.public_key(&Secp256k1::new());

        let backend = DiscoveryBackendSparse {
            weight: 0,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        };
        let address = DiscoveryBackendAddress::PublicKey(key);
        let backend1 = DiscoveryBackend {
            address,
            partition: "default".to_string(),
            backend,
        };

        let backend = DiscoveryBackendSparse {
            weight: 0,
            enabled: true,
            implementation: DiscoveryBackendImplementation::RemoteHttp,
        };
        let address = DiscoveryBackendAddress::Url(Url::parse("http://example.com/").unwrap());
        let backend2 = DiscoveryBackend {
            address,
            partition: "default".to_string(),
            backend,
        };

        let backends_expected = vec![backend1, backend2];

        let backends = r#"[{"partition":"default","address":{"publicKey":"03e7156ae33b0a208d0744199163177e909e80176e55d97a2f221ede0f934dd9ad"},"weight":0,"enabled":true,"implementation":{"type":"remoteHttp"}},{"partition":"default","address":{"url":"http://example.com/"},"weight":0,"enabled":true,"implementation":{"type":"remoteHttp"}}]"#;

        let backends: Vec<DiscoveryBackend> = serde_json::from_str(backends).unwrap();
        assert_eq!(backends_expected, backends);
    }
}
