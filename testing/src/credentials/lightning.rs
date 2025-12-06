use crate::credentials::download_credentials;
use crate::services::{IntegrationTestServices, LightningIntegrationTestServices};
use anyhow::{anyhow, Context};
use secp256k1::PublicKey;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClnRegTestLnNode {
    pub public_key: PublicKey,
    pub address: String,
    pub ca_cert_path: PathBuf,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LndRegTestLnNode {
    pub public_key: PublicKey,
    pub address: String,
    pub tls_cert_path: PathBuf,
    pub macaroon_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegTestLnNodes {
    pub cln: ClnRegTestLnNode,
    pub lnd: LndRegTestLnNode,
}

#[derive(Copy, Clone, Debug)]
enum RegTestLnNodeType {
    Cln,
    Lnd,
}

pub struct LnCredentials {
    credentials_dir: TempDir,
    lightning: LightningIntegrationTestServices,
}

impl LnCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::new();
        let credentials_dir = TempDir::new()?;
        download_credentials(credentials_dir.path(), services.credentials())?;
        Ok(Self {
            credentials_dir,
            lightning: services.lightning().clone(),
        })
    }

    pub fn get_backends(&self) -> anyhow::Result<RegTestLnNodes> {
        let credentials = self.credentials_dir.path().join("credentials");
        let base_path = credentials.as_path();

        let entries = fs::read_dir(base_path)
            .with_context(|| format!("reading directory {}", base_path.display()))?;

        let mut cln = None;
        let mut lnd = None;
        for entry in entries {
            let entry = entry
                .with_context(|| format!("reading directory entry in {}", base_path.display(),))?;

            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name() {
                Some(name) => match name.to_str() {
                    Some(s) => s,
                    None => continue,
                },
                None => continue,
            };

            let node_type = if dir_name.starts_with("cln") {
                RegTestLnNodeType::Cln
            } else if dir_name.starts_with("lnd") {
                RegTestLnNodeType::Lnd
            } else {
                continue;
            };

            let node_id_path = path.join("node_id");
            let node_id_str = fs::read_to_string(&node_id_path)
                .with_context(|| format!("reading node ID from {}", node_id_path.display(),))?;

            let node_id_hex = node_id_str.trim();
            let node_id_bytes = hex::decode(node_id_hex)
                .with_context(|| format!("decoding {} node ID to hex", node_id_path.display(),))?;

            let public_key = PublicKey::from_slice(&node_id_bytes).with_context(|| {
                format!("parsing {} public key from bytes", node_id_path.display(),)
            })?;

            match node_type {
                RegTestLnNodeType::Cln => {
                    cln = Some(Self::build_cln_node(
                        public_key,
                        &self.lightning.cln,
                        &path,
                    )?);
                }
                RegTestLnNodeType::Lnd => {
                    lnd = Some(Self::build_lnd_node(
                        public_key,
                        &self.lightning.lnd,
                        &path,
                    )?);
                }
            }

            if cln.is_some() && lnd.is_some() {
                break;
            }
        }

        Ok(RegTestLnNodes {
            cln: cln.ok_or_else(|| {
                anyhow!(
                    "cln credentials not found in {}",
                    credentials.to_string_lossy()
                )
            })?,
            lnd: lnd.ok_or_else(|| {
                anyhow!(
                    "lnd credentials not found in {}",
                    credentials.to_string_lossy()
                )
            })?,
        })
    }

    fn build_cln_node(
        public_key: PublicKey,
        address: &str,
        node_path: &Path,
    ) -> anyhow::Result<ClnRegTestLnNode> {
        let ca_cert_path = node_path.join("ca.pem");
        let ca_cert_path = ca_cert_path.canonicalize().with_context(|| {
            format!("canonicalizing CLN CA cert path {}", ca_cert_path.display(),)
        })?;
        let client_cert_path = node_path.join("client.pem");
        let client_cert_path = client_cert_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing CLN client cert path {}",
                client_cert_path.display(),
            )
        })?;
        let client_key_path = node_path.join("client-key.pem");
        let client_key_path = client_key_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing CLN client key path {}",
                client_key_path.display(),
            )
        })?;

        Ok(ClnRegTestLnNode {
            public_key,
            address: address.to_string(),
            ca_cert_path,
            client_cert_path,
            client_key_path,
        })
    }

    fn build_lnd_node(
        public_key: PublicKey,
        address: &str,
        node_path: &Path,
    ) -> anyhow::Result<LndRegTestLnNode> {
        let tls_cert_path = node_path.join("tls.cert");
        let tls_cert_path = tls_cert_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing LND TLS cert path {}",
                tls_cert_path.display(),
            )
        })?;
        let macaroon_path = node_path.join("admin.macaroon");
        let macaroon_path = macaroon_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing LND macaroon path {}",
                macaroon_path.display(),
            )
        })?;

        Ok(LndRegTestLnNode {
            public_key,
            address: address.to_string(),
            tls_cert_path,
            macaroon_path,
        })
    }
}
