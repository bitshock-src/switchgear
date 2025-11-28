use std::env;

pub const SKIP_INTEGRATION_TESTS_ENV: &str = "SWGR_SKIP_INTEGRATION_TESTS";

#[derive(Debug)]
pub struct IntegrationTestServices {
    credentials: Option<String>,
    postgres: Option<String>,
    mysql: Option<String>,
    lightning: Option<LightningIntegrationTestServices>,
}

#[derive(Debug, Clone)]
pub struct LightningIntegrationTestServices {
    pub cln: String,
    pub lnd: String,
}

impl IntegrationTestServices {
    pub fn create() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();

        let credentials = match Self::env_or_panic("CREDENTIALS_SERVER_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("CREDENTIALS_SERVER_HOSTNAME")
                    .map(|s| format!("http://{s}:{port}/credentials.tar.gz"))
            }
        };

        if credentials.is_none() {
            return Ok(Self {
                credentials,
                postgres: None,
                mysql: None,
                lightning: None,
            });
        }

        let postgres = match (&credentials, Self::env_or_panic("POSTGRES_PORT")) {
            (Some(_), Some(port)) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("POSTGRES_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
            _ => None,
        };

        let mysql = match (&credentials, Self::env_or_panic("MYSQL_PORT")) {
            (Some(_), Some(port)) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("MYSQL_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
            _ => None,
        };

        let cln = match Self::env_or_panic("CLN_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("CLN_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
        };

        let lnd = match Self::env_or_panic("LND_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("LND_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
        };

        let lightning = match (&credentials, cln, lnd) {
            (Some(_), Some(cln), Some(lnd)) => Some(LightningIntegrationTestServices { cln, lnd }),
            _ => None,
        };

        Ok(Self {
            credentials,
            postgres,
            mysql,
            lightning,
        })
    }

    fn env_or_panic(config_env: &str) -> Option<String> {
        if env::var(SKIP_INTEGRATION_TESTS_ENV).is_ok_and(|s| s.to_lowercase() == "true") {
            eprintln!("⚠️ WARNING: {SKIP_INTEGRATION_TESTS_ENV} is true, skipping integration tests for {config_env}");
            return None;
        }

        match env::var(config_env) {
            Ok(r) => Some(r),
            Err(_) => {
                panic!(
                    "

❌❌❌ ERROR ❌❌❌

Do one of:

CONFIGURE INTEGRATION TEST ENVIRONMENT

* configure integration tests - see testing/README.md
* set env {config_env} to configure the service

- or -

SKIP INTEGRATION TESTS

* set env {SKIP_INTEGRATION_TESTS_ENV}=true

❌❌❌ ERROR ❌❌❌

"
                );
            }
        }
    }

    pub fn credentials(&self) -> Option<&String> {
        self.credentials.as_ref()
    }

    pub fn postgres(&self) -> Option<&String> {
        self.postgres.as_ref()
    }

    pub fn mysql(&self) -> Option<&String> {
        self.mysql.as_ref()
    }

    pub fn lightning(&self) -> Option<&LightningIntegrationTestServices> {
        self.lightning.as_ref()
    }
}
