use anyhow::Context;
use std::env;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct IntegrationTestServices {
    postgres: String,
    mysql: String,
    credentials: String,
}

impl IntegrationTestServices {
    pub fn create() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let services_env_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
        dotenvy::from_path(&services_env_file)
            .with_context(|| format!("loading .env file {}", services_env_file.display()))?;

        let postgres_hostname = env::var("POSTGRES_HOSTNAME")?;
        let postgres_port = env::var("POSTGRES_PORT")?.parse::<u16>()?;
        let postgres = match postgres_hostname.to_socket_addrs() {
            Ok(_) => format!("{postgres_hostname}:{postgres_port}"),
            Err(_) => format!("localhost:{postgres_port}"),
        };

        let mysql_hostname = env::var("MYSQL_HOSTNAME")?;
        let mysql_port = env::var("MYSQL_PORT")?.parse::<u16>()?;
        let mysql = match mysql_hostname.to_socket_addrs() {
            Ok(_) => format!("{mysql_hostname}:{mysql_port}"),
            Err(_) => format!("localhost:{mysql_port}"),
        };

        let credentials_hostname = env::var("CREDENTIALS_SERVER_HOSTNAME")?;
        let credentials_port = env::var("CREDENTIALS_SERVER_PORT")?.parse::<u16>()?;
        let credentials = match credentials_hostname.to_socket_addrs() {
            Ok(_) => format!("{credentials_hostname}:{credentials_port}"),
            Err(_) => format!("localhost:{credentials_port}"),
        };

        Ok(Self {
            postgres,
            mysql,
            credentials,
        })
    }

    pub fn postgres(&self) -> &str {
        &self.postgres
    }

    pub fn mysql(&self) -> &str {
        &self.mysql
    }

    pub fn credentials(&self) -> &str {
        &self.credentials
    }
}
