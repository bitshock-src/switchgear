use crate::service::HasServiceErrorSource;
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
    #[serde(with = "json_bytes")]
    pub implementation: Vec<u8>,
}

mod json_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value: Value = serde_json::from_slice(bytes).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        serde_json::to_vec(&value).map_err(serde::de::Error::custom)
    }
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
