use crate::commands::error::CliError;
use crate::commands::{cli_read_to_string, cli_write_all};
use clap::{Parser, ValueEnum};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use std::collections::BTreeMap;
use std::env;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::Duration;
use switchgear_components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_components::pool::DiscoveryBackendImplementation;
use switchgear_components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_components::secrets::{SecretStore, SecretStoreConfig, SecretStoreSecretConfig};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendSparse,
    DiscoveryBackendStore,
};
use tracing::info;
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
        /// Optional backend public key, default returns all backends
        public_key: Option<String>,
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
        /// Backend public key
        public_key: String,
        /// Optional backend JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Patch an existing backend
    #[command(name = "patch")]
    Patch {
        /// Backend location public key
        public_key: String,
        /// Optional backend patch JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Enable an existing backend
    #[command(name = "enable")]
    Enable {
        /// Backend location public key
        public_key: String,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Disable an existing backend
    #[command(name = "disable")]
    Disable {
        /// Backend location public key
        public_key: String,
        #[clap(flatten)]
        client: DiscoveryBackendManagementClientConfig,
    },

    /// Delete a backend
    #[command(name = "delete", visible_alias = "rm")]
    Delete {
        /// Backend location public key
        public_key: String,
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
) -> Result<(), CliError> {
    let implementation = match ln_node_type {
        LnNodeCommandType::ClnGrpc => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url: Url::parse("https://127.0.0.1:9736")
                    .foreign_context("parsing default cln-grpc url", ErrorOrigin::Internal)?,
                domain: Some("localhost".to_string()),
                auth: ClnGrpcClientAuth {
                    ca_cert_path: PathBuf::from("/path/to/ca.pem").into(),
                    client_cert_secret: "CLIENT_CERT".to_string(),
                    client_key_secret: "CLIENT_KEY".to_string(),
                    secrets: SecretStoreConfig {
                        ttl: 300.0,
                        secrets: BTreeMap::from([
                            (
                                "CLIENT_CERT".to_string(),
                                SecretStoreSecretConfig {
                                    path: PathBuf::from("/path/to/client.pem"),
                                },
                            ),
                            (
                                "CLIENT_KEY".to_string(),
                                SecretStoreSecretConfig {
                                    path: PathBuf::from("/path/to/client-key.pem"),
                                },
                            ),
                        ]),
                    },
                },
            })
        }
        LnNodeCommandType::LndGrpc => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url: Url::parse("https://127.0.0.1:10009")
                    .foreign_context("parsing default lnd-grpc url", ErrorOrigin::Internal)?,
                domain: Some("localhost".to_string()),
                auth: LndGrpcClientAuth {
                    tls_cert_path: PathBuf::from("/path/to/tls.cert").into(),
                    macaroon_secret: "MACAROON".to_string(),
                    secrets: SecretStoreConfig {
                        ttl: 300.0,
                        secrets: BTreeMap::from([(
                            "MACAROON".to_string(),
                            SecretStoreSecretConfig {
                                path: PathBuf::from("/path/to/admin.macaroon"),
                            },
                        )]),
                    },
                },
                amp_invoice: false,
            })
        }
    };
    let backend = DiscoveryBackend {
        public_key: public_key
            .parse()
            .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?,
        backend: DiscoveryBackendSparse {
            name: name.map(String::from),
            partitions: [partition.to_string()].into(),
            weight: 1,
            enabled: false,
            implementation: serde_json::to_vec(&implementation)
                .foreign_context("serializing backend implementation", ErrorOrigin::Internal)?,
        },
    };
    let backend = serde_json::to_string_pretty(&backend)
        .foreign_context("serializing backend", ErrorOrigin::Internal)?;
    cli_write_all(output, backend.as_bytes()).with_foreign_context(
        || {
            format!(
                "writing backend to: {}",
                output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    info!("Modify this JSON file to match the {ln_node_type} node configuration");
    info!(
        "Edit auth.secrets paths so they resolve on every LNURL-service host that will serve this backend"
    );
    info!("Load it into the Discovery Service. See: swgr discovery post --help");
    Ok(())
}

pub async fn list_backends(
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_backend_client(client_configuration)?;
    let backends = client
        .get_all(None)
        .await
        .chained_context("listing backends", None)?;
    println!("# Discovery Backends");
    for backend in backends.backends.into_iter().flatten() {
        println!(
            r#"
## Public key: {}

* name: {}
* location: {}
* enabled: {}
* weight: {}
"#,
            backend.public_key,
            backend.backend.name.unwrap_or_else(|| "[null]".to_string()),
            backend.public_key,
            backend.backend.enabled,
            backend.backend.weight
        );
    }
    Ok(())
}

pub async fn get_backend(
    public_key: Option<&str>,
    output: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_backend_client(client_configuration)?;
    if let Some(public_key) = public_key {
        let public_key = public_key
            .parse()
            .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?;
        let fetched = client
            .get(&public_key)
            .await
            .with_chained_context(|| format!("fetching backend {public_key}"), None)?;
        if let Some(backend) = fetched {
            let backend = serde_json::to_string_pretty(&backend).with_foreign_context(
                || format!("serializing backend {public_key}"),
                ErrorOrigin::Internal,
            )?;
            cli_write_all(output, backend.as_bytes()).with_foreign_context(
                || {
                    format!(
                        "writing backend to: {}",
                        output.map_or_else(
                            || "stdout".to_string(),
                            |o| o.to_string_lossy().to_string()
                        )
                    )
                },
                None,
            )?;
        } else {
            return Err(CliError::internal(format!(
                "Backend {public_key} not found"
            )));
        }
    } else {
        let backends = client
            .get_all(None)
            .await
            .chained_context("listing backends", None)?;
        let backends = serde_json::to_string_pretty(&backends.backends)
            .foreign_context("serializing backends", ErrorOrigin::Internal)?;
        cli_write_all(output, backends.as_bytes()).with_foreign_context(
            || {
                format!(
                    "writing backend to: {}",
                    output
                        .map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
                )
            },
            None,
        )?;
    }

    Ok(())
}

pub async fn post_backend(
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_backend_client(client_configuration)?;
    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_foreign_context(
        || {
            format!(
                "reading backend: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |o| o.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    let backend: DiscoveryBackend = serde_json::from_str(&backend).with_foreign_context(
        || {
            format!(
                "parsing backend from: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let public_key = backend.public_key.to_string();
    if let Some(created) = client
        .post(backend)
        .await
        .with_chained_context(|| format!("posting backend {public_key}"), None)?
    {
        info!("Backend created: {}", created);
    } else {
        return Err(CliError::internal(format!(
            "Conflict. A backend already exists at: {public_key}"
        )));
    }
    Ok(())
}

pub async fn put_backend(
    public_key: &str,
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let public_key = public_key
        .parse()
        .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?;

    let client = create_backend_client(client_configuration)?;

    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_foreign_context(
        || {
            format!(
                "reading backend: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let backend: DiscoveryBackendSparse = serde_json::from_str(&backend).with_foreign_context(
        || {
            format!(
                "parsing backend from: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let backend = DiscoveryBackend {
        public_key,
        backend,
    };
    if client
        .put(backend)
        .await
        .with_chained_context(|| format!("putting backend {public_key}"), None)?
    {
        info!("Backend created: {public_key}");
    } else {
        info!("Backend updated: {public_key}");
    }
    Ok(())
}

pub async fn patch_backend(
    public_key: &str,
    backend_path: Option<&Path>,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let public_key = public_key
        .parse()
        .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?;

    let client = create_backend_client(client_configuration)?;

    let mut backend = String::new();
    cli_read_to_string(backend_path, &mut backend).with_foreign_context(
        || {
            format!(
                "reading backend: {}",
                backend_path
                    .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let backend: DiscoveryBackendPatchSparse = serde_json::from_str(&backend)
        .with_foreign_context(
            || {
                format!(
                    "parsing backend patch from: {}",
                    backend_path
                        .map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
                )
            },
            None,
        )?;
    let backend = DiscoveryBackendPatch {
        public_key,
        backend,
    };
    if client
        .patch(backend)
        .await
        .with_chained_context(|| format!("patching backend {public_key}"), None)?
    {
        info!("Backend patched: {public_key}");
    } else {
        return Err(CliError::internal(format!(
            "Backend not found: {public_key}"
        )));
    }
    Ok(())
}

pub async fn enable_backend(
    public_key: &str,
    enable: bool,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let public_key = public_key
        .parse()
        .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?;

    let client = create_backend_client(client_configuration)?;

    let backend = DiscoveryBackendPatch {
        public_key,
        backend: DiscoveryBackendPatchSparse {
            name: None,
            partitions: None,
            weight: None,
            enabled: Some(enable),
        },
    };
    if client.patch(backend).await.with_chained_context(
        || format!("patching backend {public_key} enabled={enable}"),
        None,
    )? {
        info!("Backend patched: {public_key}: enabled:{enable}");
    } else {
        return Err(CliError::internal(format!(
            "Backend not found: {public_key}"
        )));
    }
    Ok(())
}

pub async fn delete_backend(
    public_key: &str,
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_backend_client(client_configuration)?;
    let public_key = public_key
        .parse()
        .with_foreign_context(|| format!("parsing public key: {public_key}"), None)?;
    if client
        .delete(&public_key)
        .await
        .with_chained_context(|| format!("deleting backend {public_key}"), None)?
    {
        info!("Backend deleted: {public_key}");
    } else {
        return Err(CliError::internal(format!(
            "Backend not found: {public_key}"
        )));
    }
    Ok(())
}

fn create_backend_client(
    client_configuration: &DiscoveryBackendManagementClientConfig,
) -> Result<HttpDiscoveryBackendStore, CliError> {
    let base_url = match &client_configuration.base_url {
        None => {
            let base_url = env::var("DISCOVERY_STORE_HTTP_BASE_URL").foreign_context(
                "Missing DISCOVERY_STORE_HTTP_BASE_URL",
                ErrorOrigin::Downstream,
            )?;
            Url::parse(&base_url).with_foreign_context(|| format!("parsing {base_url}"), None)?
        }
        Some(base_url) => base_url.clone(),
    };
    let authorization_path = match &client_configuration.authorization_path {
        None => {
            let authorization_path = env::var("DISCOVERY_STORE_HTTP_AUTHORIZATION")
                .foreign_context(
                    "Missing DISCOVERY_STORE_HTTP_AUTHORIZATION",
                    ErrorOrigin::Downstream,
                )?;
            PathBuf::from(authorization_path)
        }
        Some(authorization_path) => authorization_path.clone(),
    };

    let trusted_roots_path = match &client_configuration.trusted_roots {
        None => env::var("DISCOVERY_STORE_HTTP_TRUSTED_ROOTS")
            .ok()
            .map(PathBuf::from),
        Some(trusted_roots_path) => Some(trusted_roots_path.clone()),
    };

    let trusted_roots = if let Some(trusted_roots_path) = trusted_roots_path {
        CertificateDer::pem_file_iter(&trusted_roots_path)
            .with_foreign_context(
                || format!("parsing root certificate: {}", trusted_roots_path.display()),
                None,
            )?
            .collect::<Result<Vec<_>, _>>()
            .with_foreign_context(
                || format!("parsing root certificate: {}", trusted_roots_path.display()),
                None,
            )?
    } else {
        vec![]
    };

    let secret_store = SecretStore::create(&SecretStoreConfig {
        ttl: 300.0,
        secrets: BTreeMap::from([(
            "ADHOC".to_owned(),
            SecretStoreSecretConfig {
                path: authorization_path,
            },
        )]),
    });

    HttpDiscoveryBackendStore::create(
        base_url,
        Duration::from_secs(1),
        Duration::from_secs(1),
        &trusted_roots,
        secret_store,
        "ADHOC",
    )
    .chained_context("creating http discovery backend store client", None)
}
