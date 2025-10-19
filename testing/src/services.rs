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

        let postgres_port = env::var("POSTGRES_PORT")?.parse::<u16>()?;
        let postgres = format!(
            "{}.services_network:{postgres_port}",
            env::var("POSTGRES_HOSTNAME")?
        );
        eprintln!("attempting to resolve: {postgres}");
        let postgres = postgres
            .to_socket_addrs()
            .map_or_else(|_| format!("localhost:{postgres_port}"), |_| postgres);

        let mysql_port = env::var("MYSQL_PORT")?.parse::<u16>()?;
        let mysql = format!(
            "{}.services_network:{mysql_port}",
            env::var("MYSQL_HOSTNAME")?
        );
        eprintln!("attempting to resolve: {mysql}");
        let mysql = mysql
            .to_socket_addrs()
            .map_or_else(|_| format!("localhost:{mysql_port}"), |_| mysql);

        let credentials_port = env::var("CREDENTIALS_SERVER_PORT")?.parse::<u16>()?;
        let credentials = format!(
            "{}.services_network:{credentials_port}",
            env::var("CREDENTIALS_SERVER_HOSTNAME")?
        );
        eprintln!("attempting to resolve: {credentials}");
        let credentials = credentials
            .to_socket_addrs()
            .map_or_else(|_| format!("localhost:{credentials_port}"), |_| credentials);

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

#[cfg(test)]
mod tests {
    use crate::services::IntegrationTestServices;

    #[test]
    pub fn test_services() {
        let services = IntegrationTestServices::create().unwrap();
        eprintln!("{:?}", services);
    }
}
