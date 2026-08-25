use crate::credentials::download_credentials;
use crate::services::{IntegrationTestServices, OtelIntegrationTestServices};
use anyhow::{Context, anyhow};
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OtelCollector {
    pub grpc_endpoint: String,
    pub jaeger_query_endpoint: String,
    pub ca_cert_path: PathBuf,
    pub bearer_token_path: PathBuf,
    pub client_cert_path: Option<PathBuf>,
    pub client_key_path: Option<PathBuf>,
}

pub struct OtelCredentials {
    credentials_dir: TempDir,
    otel: OtelIntegrationTestServices,
}

impl OtelCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::new();
        let credentials_dir = TempDir::new()?;
        download_credentials(credentials_dir.path(), services.credentials())?;
        Ok(Self {
            credentials_dir,
            otel: services.otel().clone(),
        })
    }

    pub fn get_collector(&self) -> anyhow::Result<OtelCollector> {
        let base = self
            .credentials_dir
            .path()
            .join("credentials")
            .join("otel-collector");

        if !base.is_dir() {
            return Err(anyhow!(
                "otel-collector credentials not found in {}",
                base.display()
            ));
        }

        let ca_cert_path = base.join("ca.pem");
        let ca_cert_path = ca_cert_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing OTEL CA cert path {}",
                ca_cert_path.display()
            )
        })?;

        let bearer_token_path = base.join("token");
        let bearer_token_path = bearer_token_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing OTEL bearer token path {}",
                bearer_token_path.display()
            )
        })?;

        let client_cert_path = base.join("client-cert.pem");
        let client_cert_path = client_cert_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing OTEL client cert path {}",
                client_cert_path.display()
            )
        })?;

        let client_key_path = base.join("client-key.pem");
        let client_key_path = client_key_path.canonicalize().with_context(|| {
            format!(
                "canonicalizing OTEL client key path {}",
                client_key_path.display()
            )
        })?;

        Ok(OtelCollector {
            grpc_endpoint: self.otel.grpc_endpoint.clone(),
            jaeger_query_endpoint: self.otel.jaeger_query_endpoint.clone(),
            ca_cert_path,
            bearer_token_path,
            client_cert_path: Some(client_cert_path),
            client_key_path: Some(client_key_path),
        })
    }
}
