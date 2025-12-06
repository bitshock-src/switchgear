use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClnGrpcDiscoveryBackendImplementation {
    pub url: Url,
    pub domain: Option<String>,
    pub auth: ClnGrpcClientAuth,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum ClnGrpcClientAuth {
    Path(ClnGrpcClientAuthPath),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClnGrpcClientAuthPath {
    pub ca_cert_path: Option<PathBuf>,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
}
