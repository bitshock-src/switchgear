use crate::commands::{cli_read_to_string, cli_write_all};
use anyhow::{anyhow, bail, Context};
use clap::{Parser, ValueEnum};
use log::info;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use std::{env, fs};
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendImplementation,
    DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendRest,
    DiscoveryBackendSparse, DiscoveryBackendStore,
};
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_service::components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_service::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcClientAuthPath, LndGrpcDiscoveryBackendImplementation,
};
use url::Url;

#[derive(Parser, Debug)]
pub enum DiscoveryBackendManagementCommands {
    /// Generate backend JSON
    #[command(name = "new")]
    New {
        /// Lighting Node type
        node_type: LnNodeCommandType,

        /// Lighting Node public key
        public_key: String,

        /// Partition binding name
        #[arg(short, long)]
        partition: String,

        /// Optional backend name
        #[arg(short, long)]
        name: Option<String>,

        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List all backends
    #[command(name = "ls")]
    List {
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Get a backend
    #[command(name = "get")]
    Get {
        /// Optional backend location address, default returns all backends
        address: Option<String>,
        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Load a new backend
    #[command(name = "post")]
    Post {
        /// Optional backend JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Update or create a backend
    #[command(name = "put")]
    Put {
        /// Backend location address
        address: String,
        /// Optional backend JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Patch an existing backend
    #[command(name = "patch")]
    Patch {
        /// Backend location address
        address: String,
        /// Optional backend patch JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Enable an existing backend
    #[command(name = "enable")]
    Enable {
        /// Backend location address
        address: String,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Disable an existing backend
    #[command(name = "disable")]
    Disable {
        /// Backend location address
        address: String,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Delete a backend
    #[command(name = "delete", visible_alias = "rm")]
    Delete {
        /// Backend location address
        address: String,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },
}

#[derive(Parser, Debug)]
pub struct DiscoveryBackendManagementClientConfig {
    /// Optional Discovery Service base Url. Also set with DISCOVERY_STORE_HTTP_BASE_URL env var
    #[clap(short, long)]
    base_url: Option<Url>,
    /// Optional Discovery Service authorization token file path. Also set with DISCOVERY_STORE_HTTP_AUTHORIZATION env var
    #[clap(short, long)]
    authorization_path: Option<PathBuf>,
    /// Optional Discovery Service trusted roots file path, in pem format. Also set with DISCOVERY_STORE_HTTP_TRUSTED_ROOTS env var
    #[clap(short, long)]
    trusted_roots: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum LnNodeCommandType {
    #[value(name = "cln-grpc")]
    ClnGrpc,
    #[value(name = "lnd-grpc")]
    LndGrpc,
}

impl Display for LnNodeCommandType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LnNodeCommandType::ClnGrpc => write!(f, "CLN gRPC"),
            LnNodeCommandType::LndGrpc => write!(f, "LND gRPC"),
        }
    }
}

pub fn new_backend(
    ln_node_type: LnNodeCommandType,
    public_key: &str,
    name: Option<&str>,
    partition: &str,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    let implementation = match ln_node_type {
        LnNodeCommandType::ClnGrpc => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url: Url::parse("https://127.0.0.1:9736")?,
                domain: Some("localhost".to_string()),
                auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                    ca_cert_path: PathBuf::from("/path/to/ca.pem").into(),
                    client_cert_path: PathBuf::from("/path/to/client.pem"),
                    client_key_path: PathBuf::from("/path/to/client-key.pem"),
                }),
            })
        }
        LnNodeCommandType::LndGrpc => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url: Url::parse("https://127.0.0.1:10009")?,
                domain: Some("localhost".to_string()),
                auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                    tls_cert_path: PathBuf::from("/path/to/tls.cert").into(),
                    macaroon_path: PathBuf::from("/path/to/admin.macaroon"),
                }),
                amp_invoice: false,
            })
        }
    };
    let backend = DiscoveryBackend {
        address: DiscoveryBackendAddress::PublicKey(public_key.parse()?),
        backend: DiscoveryBackendSparse {
            name: name.map(String::from),
            partitions: [partition.to_string()].into(),
            weight: 1,
            enabled: false,
            implementation,
        },
    };
    let backend = serde_json::to_string_pretty(&backend).with_context(|| "serializing backend")?;
    cli_write_all(output, backend.as_bytes()).with_context(|| {
        format!(
            "writing backend to: {}",
            output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

    info!("Modify this JSON file to match the {ln_node_type} node configuration");
    info!("Load it into the Discovery Service. See: swgr discovery post --help");
    Ok(())
}

pub async fn list_backends(
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;
    let backends = client.get_all().await?;
    println!("# Discovery Backends");
    for backend in backends {
        println!(
            r#"
## Address: {}

* name: {}
* location: {}
* enabled: {}
* weight: {}
"#,
            backend.address,
            backend.backend.name.unwrap_or_else(|| "[null]".to_string()),
            backend.address.encoded(),
            backend.backend.enabled,
            backend.backend.weight
        );
    }
    Ok(())
}

pub async fn get_backend(
    address: Option<&str>,
    output: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;
    if let Some(address) = address {
        let address_parsed = DiscoveryBackendAddress::from_str(address)
            .with_context(|| format!("reading address: {address}"))?;
        if let Some(backend) = client.get(&address_parsed).await? {
            let backend = DiscoveryBackendRest {
                location: backend.address.encoded(),
                backend,
            };
            let backend = serde_json::to_string_pretty(&backend)
                .with_context(|| format!("serializing backend {address}"))?;
            cli_write_all(output, backend.as_bytes()).with_context(|| {
                format!(
                    "writing backend to: {}",
                    output
                        .map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
                )
            })?;
        } else {
            bail!("Backend {address} not found");
        }
    } else {
        let backends = client.get_all().await?;
        let backends = backends
            .into_iter()
            .map(|backend| DiscoveryBackendRest {
                location: backend.address.encoded(),
                backend,
            })
            .collect::<Vec<_>>();
        let backends =
            serde_json::to_string_pretty(&backends).with_context(|| "serializing backends")?;
        cli_write_all(output, backends.as_bytes()).with_context(|| {
            format!(
                "writing backend to: {}",
                output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
            )
        })?;
    }

    Ok(())
}

pub async fn post_backend(
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;
    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_context(|| {
        format!(
            "reading backend: {}",
            backend_path.map_or_else(|| "stdin".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

    let backend: DiscoveryBackend = serde_json::from_str(&backend).with_context(|| {
        format!(
            "parsing backend from: {}",
            backend_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let address_encoded = backend.address.encoded();
    if let Some(created) = client.post(backend).await? {
        info!("Backend created: {}", created.encoded());
    } else {
        bail!("Conflict. A backend already exists at: {address_encoded}",);
    }
    Ok(())
}

pub async fn put_backend(
    address: &str,
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;

    let address = DiscoveryBackendAddress::from_str(address)
        .with_context(|| format!("reading address: {address}"))?;

    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_context(|| {
        format!(
            "reading backend: {}",
            backend_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let backend: DiscoveryBackendSparse = serde_json::from_str(&backend).with_context(|| {
        format!(
            "parsing backend from: {}",
            backend_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let address_encoded = address.encoded();
    let backend = DiscoveryBackend { address, backend };
    if client.put(backend).await? {
        info!("Backend created: {address_encoded}");
    } else {
        info!("Backend updated: {address_encoded}");
    }
    Ok(())
}

pub async fn patch_backend(
    address: &str,
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;

    let address = DiscoveryBackendAddress::from_str(address)
        .with_context(|| format!("reading address: {address}"))?;

    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_context(|| {
        format!(
            "reading backend: {}",
            backend_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let backend: DiscoveryBackendPatchSparse =
        serde_json::from_str(&backend).with_context(|| {
            format!(
                "parsing backend patch from: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        })?;
    let address_encoded = address.encoded();
    let backend = DiscoveryBackendPatch { address, backend };
    if client.patch(backend).await? {
        info!("Backend patched: {address_encoded}");
    } else {
        bail!("Backend not found: {address_encoded}");
    }
    Ok(())
}

pub async fn enable_backend(
    address: &str,
    enable: bool,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;

    let address = DiscoveryBackendAddress::from_str(address)
        .with_context(|| format!("reading address: {address}"))?;

    let address_encoded = address.encoded();
    let backend = DiscoveryBackendPatch {
        address,
        backend: DiscoveryBackendPatchSparse {
            name: None,
            partitions: None,
            weight: None,
            enabled: Some(enable),
        },
    };
    if client.patch(backend).await? {
        info!("Backend patched: {address_encoded}: enabled:{enable}");
    } else {
        bail!("Backend not found: {address_encoded}");
    }
    Ok(())
}

pub async fn delete_backend(
    address: &str,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_backend_client(client_configuration)?;
    let address = DiscoveryBackendAddress::from_str(address)
        .with_context(|| format!("reading address: {address}"))?;
    if client.delete(&address).await? {
        info!("Backend deleted: {}", address.encoded());
    } else {
        bail!("Backend not found: {}", address.encoded());
    }
    Ok(())
}

fn create_backend_client(
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> anyhow::Result<HttpDiscoveryBackendStore> {
    let base_url = match &client_configuration.base_url {
        None => {
            let base_url = env::var("DISCOVERY_STORE_HTTP_BASE_URL")
                .map_err(|_| anyhow!("Missing DISCOVERY_STORE_HTTP_BASE_URL"))?;
            Url::parse(&base_url).with_context(|| format!("parsing {base_url}"))?
        }
        Some(base_url) => base_url.clone(),
    };
    let authorization_path = match &client_configuration.authorization_path {
        None => {
            let authorization_path = env::var("DISCOVERY_STORE_HTTP_AUTHORIZATION")
                .map_err(|_| anyhow!("Missing DISCOVERY_STORE_HTTP_AUTHORIZATION"))?;
            PathBuf::from(authorization_path)
        }
        Some(authorization_path) => authorization_path.clone(),
    };
    let authorization = fs::read_to_string(&authorization_path).with_context(|| {
        format!(
            "reading authorization file: {}",
            authorization_path.to_string_lossy()
        )
    })?;

    let trusted_roots_path = match &client_configuration.trusted_roots {
        None => env::var("DISCOVERY_STORE_HTTP_TRUSTED_ROOTS")
            .map_err(|_| anyhow!("Missing DISCOVERY_STORE_HTTP_TRUSTED_ROOTS"))
            .ok()
            .map(PathBuf::from),
        Some(trusted_roots_path) => Some(trusted_roots_path.clone()),
    };

    let trusted_roots = if let Some(trusted_roots_path) = trusted_roots_path {
        CertificateDer::pem_file_iter(&trusted_roots_path)
            .with_context(|| format!("parsing root certificate: {}", trusted_roots_path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!("parsing root certificate: {}", trusted_roots_path.display())
            })?
    } else {
        vec![]
    };

    Ok(HttpDiscoveryBackendStore::create(
        base_url,
        Duration::from_secs(1),
        Duration::from_secs(1),
        &trusted_roots,
        authorization,
    )?)
}
