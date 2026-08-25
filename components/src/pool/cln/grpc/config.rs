use crate::secrets::SecretStoreConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClnGrpcDiscoveryBackendImplementation {
    pub url: Url,
    pub domain: Option<String>,
    pub auth: ClnGrpcClientAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClnGrpcClientAuth {
    pub ca_cert_path: Option<PathBuf>,
    pub client_cert_secret: String,
    pub client_key_secret: String,
    pub secrets: SecretStoreConfig,
}
