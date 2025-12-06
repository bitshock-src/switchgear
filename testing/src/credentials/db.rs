use crate::credentials::download_credentials;
use crate::services::IntegrationTestServices;
use anyhow::{anyhow, Context};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TestDatabase {
    pub address: String,
    pub ca_cert_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TestDatabases {
    pub postgres: TestDatabase,
    pub mysql: TestDatabase,
}

pub struct DbCredentials {
    credentials_dir: TempDir,
    postgres: String,
    mysql: String,
}

impl DbCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::new();

        let credentials_dir = TempDir::new()?;
        download_credentials(credentials_dir.path(), services.credentials())?;
        Ok(Self {
            credentials_dir,
            postgres: services.postgres().to_string(),
            mysql: services.mysql().to_string(),
        })
    }

    pub fn get_databases(&self) -> anyhow::Result<TestDatabases> {
        let credentials = self.credentials_dir.path().join("credentials");
        let base_path = credentials.as_path();

        let entries = fs::read_dir(base_path)
            .with_context(|| format!("reading directory {}", base_path.display()))?;

        let mut postgres = None;
        let mut mysql = None;
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

            if dir_name == "postgres" {
                postgres = Some(TestDatabase {
                    address: self.postgres.to_string(),
                    ca_cert_path: path.join("server.pem"),
                });
            }

            if dir_name == "mysql" {
                mysql = Some(TestDatabase {
                    address: self.mysql.to_string(),
                    ca_cert_path: path.join("server.pem"),
                });
            }

            if postgres.is_some() && mysql.is_some() {
                break;
            }
        }

        Ok(TestDatabases {
            postgres: postgres.ok_or_else(|| {
                anyhow!(
                    "postgres credentials not found in {}",
                    self.credentials_dir.path().to_string_lossy()
                )
            })?,
            mysql: mysql.ok_or_else(|| {
                anyhow!(
                    "mysql credentials not found in {}",
                    self.credentials_dir.path().to_string_lossy()
                )
            })?,
        })
    }
}
