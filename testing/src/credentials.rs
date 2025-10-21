use crate::services::{IntegrationTestServices, LightningIntegrationTestServices};
use anyhow::Context;
use flate2::read::GzDecoder;
use secp256k1::PublicKey;
use std::fs;
use std::path::{Path, PathBuf};
use tar::Archive;
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClnRegTestLnNode {
    pub public_key: PublicKey,
    pub address: String,
    pub ca_cert_path: PathBuf,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
    pub sni: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LndRegTestLnNode {
    pub public_key: PublicKey,
    pub address: String,
    pub tls_cert_path: PathBuf,
    pub macaroon_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegTestLnNode {
    Cln(ClnRegTestLnNode),
    Lnd(LndRegTestLnNode),
}

impl RegTestLnNode {
    pub fn public_key(&self) -> &PublicKey {
        match self {
            RegTestLnNode::Cln(cln) => &cln.public_key,
            RegTestLnNode::Lnd(lnd) => &lnd.public_key,
        }
    }

    pub fn address(&self) -> &str {
        match self {
            RegTestLnNode::Cln(cln) => &cln.address,
            RegTestLnNode::Lnd(lnd) => &lnd.address,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            RegTestLnNode::Cln(_) => "cln",
            RegTestLnNode::Lnd(_) => "lnd",
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RegTestLnNodeType {
    Cln,
    Lnd,
}

pub struct LnCredentials {
    inner: Option<LnCredentialsInner>,
}

struct LnCredentialsInner {
    credentials_dir: TempDir,
    lightning: LightningIntegrationTestServices,
}

impl LnCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::create()?;
        let inner = match services.lightning() {
            None => None,
            Some(lightning) => {
                let credentials_dir = TempDir::new()?;
                Self::download_credentials(credentials_dir.path(), &lightning.credentials)?;
                Some(LnCredentialsInner {
                    credentials_dir,
                    lightning: lightning.clone(),
                })
            }
        };
        Ok(Self { inner })
    }

    fn download_credentials(credentials_dir: &Path, credentials_url: &str) -> anyhow::Result<()> {
        let download_path = credentials_dir.join("credentials.tar.gz");
        let response = ureq::get(credentials_url)
            .call()
            .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

        let bytes = response
            .into_body()
            .read_to_vec()
            .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

        fs::write(&download_path, &bytes)
            .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

        let tar_gz = fs::File::open(&download_path)
            .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive
            .unpack(credentials_dir)
            .with_context(|| format!("Downloading credentials from {}", credentials_url))?;
        Ok(())
    }

    pub fn get_backends(&self) -> anyhow::Result<Vec<RegTestLnNode>> {
        let inner = match &self.inner {
            None => return Ok(vec![]),
            Some(inner) => inner,
        };

        let credentials = inner.credentials_dir.path().join("credentials");
        let base_path = credentials.as_path();

        let entries = fs::read_dir(base_path)
            .with_context(|| format!("reading directory {}", base_path.display()))?;

        let mut backends = Vec::new();

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

            let node = match node_type {
                RegTestLnNodeType::Cln => RegTestLnNode::Cln(Self::build_cln_node(
                    public_key,
                    &inner.lightning.cln,
                    &path,
                )?),
                RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(Self::build_lnd_node(
                    public_key,
                    &inner.lightning.lnd,
                    &path,
                )?),
            };

            backends.push(node);
        }

        Ok(backends)
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
            sni: "localhost".to_string(),
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
