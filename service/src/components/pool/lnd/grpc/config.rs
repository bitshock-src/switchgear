use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LndGrpcDiscoveryBackendImplementation {
    pub url: Url,
    pub domain: Option<String>,
    pub auth: LndGrpcClientAuth,
    pub amp_invoice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum LndGrpcClientAuth {
    Path(LndGrpcClientAuthPath),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LndGrpcClientAuthPath {
    pub tls_cert_path: PathBuf,
    pub macaroon_path: PathBuf,
}
