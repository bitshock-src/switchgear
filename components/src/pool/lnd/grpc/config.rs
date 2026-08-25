use crate::secrets::SecretStoreConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LndGrpcDiscoveryBackendImplementation {
    pub url: Url,
    pub domain: Option<String>,
    pub auth: LndGrpcClientAuth,
    pub amp_invoice: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LndGrpcClientAuth {
    pub tls_cert_path: Option<PathBuf>,
    pub macaroon_secret: String,
    pub secrets: SecretStoreConfig,
}
