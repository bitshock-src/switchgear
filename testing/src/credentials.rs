use anyhow::Context;
use flate2::read::GzDecoder;
use secp256k1::PublicKey;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tar::Archive;
use tempfile::TempDir;

const CREDENTIALS_URL_ENV: &str = "LNURL_BALANCER_CREDENTIALS_URL";
const SKIP_INTEGRATION_TESTS_ENV: &str = "LNURL_SKIP_INTEGRATION_TESTS";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClnRegTestLnNode {
    pub public_key: PublicKey,
    pub address: RegTestLnNodeAddress,
    pub ca_cert_path: PathBuf,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
    pub sni: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LndRegTestLnNode {
    pub public_key: PublicKey,
    pub address: RegTestLnNodeAddress,
    pub tls_cert_path: PathBuf,
    pub macaroon_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegTestLnNodeAddress {
    Inet(SocketAddr),
    Path(Vec<u8>),
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

    pub fn address(&self) -> &RegTestLnNodeAddress {
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
    credentials_dir: Option<TempDir>,
}

impl LnCredentials {
    pub fn create() -> anyhow::Result<Self> {
        dotenvy::dotenv()?;

        let credentials_dir =
            if env::var(SKIP_INTEGRATION_TESTS_ENV).is_ok_and(|s| s.to_lowercase() == "true") {
                eprintln!("⚠️ WARNING: {SKIP_INTEGRATION_TESTS_ENV} is true, skipping tests");
                None
            } else {
                let credentials_url = match env::var(CREDENTIALS_URL_ENV) {
                    Ok(url) => url,
                    Err(_) => {
                        panic!(
                            "

❌❌❌ ERROR ❌❌❌

Do one of:

1. configure test environment (see testing/README.md) and ensure credentials server is running
2. set env {CREDENTIALS_URL_ENV} to the URL of your credentials server
3. set env {SKIP_INTEGRATION_TESTS_ENV}=true to skip integration tests

❌❌❌ ERROR ❌❌❌

                "
                        );
                    }
                };

                let credentials_dir = TempDir::new()?;
                Self::download_credentials(credentials_dir.path(), &credentials_url)?;
                Some(credentials_dir)
            };

        Ok(Self { credentials_dir })
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
        let base_path = match &self.credentials_dir {
            None => return Ok(vec![]),
            Some(base_path) => base_path.path(),
        };

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

            let address = Self::read_node_address(&path)?;

            let node = match node_type {
                RegTestLnNodeType::Cln => {
                    RegTestLnNode::Cln(Self::build_cln_node(public_key, address, &path)?)
                }
                RegTestLnNodeType::Lnd => {
                    RegTestLnNode::Lnd(Self::build_lnd_node(public_key, address, &path)?)
                }
            };

            backends.push(node);
        }

        Ok(backends)
    }

    fn read_node_address(node_path: &Path) -> anyhow::Result<RegTestLnNodeAddress> {
        let address_file = node_path.join("address.txt");

        let address_content = fs::read_to_string(&address_file)
            .with_context(|| format!("reading {} address", address_file.display(),))?;

        let address_str = address_content.trim();
        let socket_addr = SocketAddr::from_str(address_str).with_context(|| {
            format!(
                "parsing {} address '{}'",
                address_file.display(),
                address_str
            )
        })?;

        Ok(RegTestLnNodeAddress::Inet(socket_addr))
    }

    fn build_cln_node(
        public_key: PublicKey,
        address: RegTestLnNodeAddress,
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
            address,
            ca_cert_path,
            client_cert_path,
            client_key_path,
            sni: "localhost".to_string(),
        })
    }

    fn build_lnd_node(
        public_key: PublicKey,
        address: RegTestLnNodeAddress,
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
            address,
            tls_cert_path,
            macaroon_path,
        })
    }
}
