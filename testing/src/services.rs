use anyhow::Context;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

pub struct IntegrationTestServices {
    postgres: SocketAddr,
    mysql: SocketAddr,
}

impl IntegrationTestServices {
    pub fn create() -> anyhow::Result<Self> {
        let localhost = match "host.docker.internal".to_socket_addrs() {
            Ok(mut addr) => addr
                .next()
                .map(|addr| addr.ip())
                .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            Err(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        };
        let _ = dotenvy::dotenv();
        let services_env_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
        dotenvy::from_path(&services_env_file)
            .with_context(|| format!("loading .env file {}", services_env_file.display()))?;
        let postgres = SocketAddr::new(localhost, env::var("POSTGRES_PORT")?.parse()?);
        let mysql = SocketAddr::new(localhost, env::var("MYSQL_PORT")?.parse()?);
        Ok(Self { postgres, mysql })
    }

    pub fn postgres(&self) -> SocketAddr {
        self.postgres
    }

    pub fn mysql(&self) -> SocketAddr {
        self.mysql
    }
}
