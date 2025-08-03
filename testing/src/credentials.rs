use anyhow::Context;
use secp256k1::PublicKey;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const CREDENTIALS_PATH_ENV: &str = "LNURL_BALANCER_CREDENTIALS_PATH";
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

pub fn get_backends() -> anyhow::Result<Vec<RegTestLnNode>> {
    if env::var(SKIP_INTEGRATION_TESTS_ENV).is_ok_and(|s| s.to_lowercase() == "true") {
        eprintln!("⚠️ WARNING: {SKIP_INTEGRATION_TESTS_ENV} is true, skipping tests");
        return Ok(Vec::new());
    }

    match get_backends_path() {
        None => {
            panic!(
                "
            
❌❌❌ ERROR ❌❌❌

{CREDENTIALS_PATH_ENV} is not set. Do one of:

1. configure test environment (see testing/README.md) and set {CREDENTIALS_PATH_ENV}
2. set env {SKIP_INTEGRATION_TESTS_ENV}=true to skip integration tests

❌❌❌ ERROR ❌❌❌

            "
            );
        }
        Some(base_path) => get_backends_from_path(base_path),
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RegTestLnNodeType {
    Cln,
    Lnd,
}

fn get_backends_from_path(base_path: PathBuf) -> anyhow::Result<Vec<RegTestLnNode>> {
    let entries = fs::read_dir(&base_path)
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
                None => continue, // Skip non-UTF8 names
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

        // Read node ID
        let node_id_path = path.join("node_id");
        let node_id_str = fs::read_to_string(&node_id_path)
            .with_context(|| format!("reading node ID from {}", node_id_path.display(),))?;

        let node_id_hex = node_id_str.trim();
        let node_id_bytes = hex::decode(node_id_hex)
            .with_context(|| format!("decoding {} node ID to hex", node_id_path.display(),))?;

        let public_key = PublicKey::from_slice(&node_id_bytes).with_context(|| {
            format!("parsing {} public key from bytes", node_id_path.display(),)
        })?;

        // Read address from address.txt
        let address = read_node_address(&path)?;

        let node = match node_type {
            RegTestLnNodeType::Cln => {
                RegTestLnNode::Cln(build_cln_node(public_key, address, &path)?)
            }
            RegTestLnNodeType::Lnd => {
                RegTestLnNode::Lnd(build_lnd_node(public_key, address, &path)?)
            }
        };

        backends.push(node);
    }

    Ok(backends)
}

fn get_backends_path() -> Option<PathBuf> {
    let _ = dotenvy::dotenv();
    env::var(CREDENTIALS_PATH_ENV).ok().map(PathBuf::from)
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
    let ca_cert_path = ca_cert_path
        .canonicalize()
        .with_context(|| format!("canonicalizing CLN CA cert path {}", ca_cert_path.display(),))?;
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
