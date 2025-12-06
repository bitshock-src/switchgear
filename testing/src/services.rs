use std::env;

const TESTING_ENV_FILE_PATH: &str = "./testing.env";

#[derive(Debug)]
pub struct IntegrationTestServices {
    credentials: String,
    postgres: String,
    mysql: String,
    lightning: LightningIntegrationTestServices,
}

#[derive(Debug, Clone)]
pub struct LightningIntegrationTestServices {
    pub cln: String,
    pub lnd: String,
}

impl Default for IntegrationTestServices {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegrationTestServices {
    pub fn new() -> Self {
        let _ = dotenvy::from_filename(TESTING_ENV_FILE_PATH);

        let credentials = format!(
            "http://{}:{}/credentials.tar.gz",
            Self::env_or_panic("CREDENTIALS_SERVER_HOSTNAME"),
            Self::env_or_panic("CREDENTIALS_SERVER_PORT")
        );

        let postgres = format!(
            "{}:{}",
            Self::env_or_panic("POSTGRES_HOSTNAME"),
            Self::env_or_panic("POSTGRES_PORT")
        );

        let mysql = format!(
            "{}:{}",
            Self::env_or_panic("MYSQL_HOSTNAME"),
            Self::env_or_panic("MYSQL_PORT")
        );

        let cln = format!(
            "{}:{}",
            Self::env_or_panic("CLN_HOSTNAME"),
            Self::env_or_panic("CLN_PORT")
        );

        let lnd = format!(
            "{}:{}",
            Self::env_or_panic("LND_HOSTNAME"),
            Self::env_or_panic("LND_PORT")
        );

        Self {
            credentials,
            postgres,
            mysql,
            lightning: LightningIntegrationTestServices { cln, lnd },
        }
    }

    fn env_or_panic(config_env: &str) -> String {
        env::var(config_env).unwrap_or_else(|_| {
            panic!(
                "

❌ INVALID INTEGRATION TEST ENVIRONMENT ❌

Env var '{config_env}' is not set.

See testing/README.md to configure integration tests and services.

",
            )
        })
    }

    pub fn credentials(&self) -> &str {
        &self.credentials
    }

    pub fn postgres(&self) -> &str {
        &self.postgres
    }

    pub fn mysql(&self) -> &str {
        &self.mysql
    }

    pub fn lightning(&self) -> &LightningIntegrationTestServices {
        &self.lightning
    }
}
