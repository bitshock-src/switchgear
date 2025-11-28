use crate::credentials::download_credentials;
use crate::services::IntegrationTestServices;
use anyhow::Context;
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
    pub postgres: Option<TestDatabase>,
    pub mysql: Option<TestDatabase>,
}

pub struct DbCredentials {
    inner: Option<DbCredentialsInner>,
}

struct DbCredentialsInner {
    credentials_dir: TempDir,
    postgres: String,
    mysql: String,
}

impl DbCredentials {
    pub fn create() -> anyhow::Result<Self> {
        let services = IntegrationTestServices::create()?;

        let inner = match (
            services.credentials(),
            services.postgres(),
            services.mysql(),
        ) {
            (Some(credentials), Some(postgres), Some(mysql)) => {
                let credentials_dir = TempDir::new()?;
                download_credentials(credentials_dir.path(), credentials)?;
                Some(DbCredentialsInner {
                    credentials_dir,
                    postgres: postgres.to_string(),
                    mysql: mysql.to_string(),
                })
            }
            _ => None,
        };
        Ok(Self { inner })
    }

    pub fn get_databases(&self) -> anyhow::Result<TestDatabases> {
        let inner = match &self.inner {
            None => {
                return Ok(TestDatabases {
                    postgres: None,
                    mysql: None,
                })
            }
            Some(inner) => inner,
        };

        let credentials = inner.credentials_dir.path().join("credentials");
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
                    address: inner.postgres.to_string(),
                    ca_cert_path: path.join("server.pem"),
                });
            }

            if dir_name == "mysql" {
                mysql = Some(TestDatabase {
                    address: inner.mysql.to_string(),
                    ca_cert_path: path.join("server.pem"),
                });
            }

            if postgres.is_some() && mysql.is_some() {
                break;
            }
        }

        Ok(TestDatabases { postgres, mysql })
    }
}
