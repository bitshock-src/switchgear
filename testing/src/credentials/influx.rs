use crate::credentials::download_credentials;
use crate::services::IntegrationTestServices;
use anyhow::{Context, anyhow};
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Clone, Debug)]
pub struct Influx {
    pub query_endpoint: String,
    pub token: String,
    pub token_path: PathBuf,
}

pub struct InfluxCredentials {
    credentials_dir: TempDir,
    query_endpoint: String,
}

impl InfluxCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::new();
        let credentials_dir = TempDir::new()?;
        download_credentials(credentials_dir.path(), services.credentials())?;
        Ok(Self {
            credentials_dir,
            query_endpoint: services.influx().query_endpoint.clone(),
        })
    }

    pub fn get_influx(&self) -> anyhow::Result<Influx> {
        let token_path = self
            .credentials_dir
            .path()
            .join("credentials")
            .join("influxdb")
            .join("token");
        let token_path = token_path.canonicalize().with_context(|| {
            format!("canonicalizing influx token path {}", token_path.display())
        })?;
        let token = std::fs::read_to_string(&token_path)
            .with_context(|| format!("reading influx token {}", token_path.display()))?
            .trim()
            .to_owned();
        if token.is_empty() {
            return Err(anyhow!("influx token at {} is empty", token_path.display()));
        }
        Ok(Influx {
            query_endpoint: self.query_endpoint.clone(),
            token,
            token_path,
        })
    }
}
