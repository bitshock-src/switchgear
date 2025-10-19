use std::env;

pub const SKIP_INTEGRATION_TESTS_ENV: &str = "LNURL_SKIP_INTEGRATION_TESTS";

#[derive(Debug)]
pub struct IntegrationTestServices {
    postgres: Option<String>,
    mysql: Option<String>,
    credentials: Option<String>,
}

impl IntegrationTestServices {
    pub fn create() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();

        let postgres = match Self::env_or_panic("POSTGRES_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("POSTGRES_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
        };

        let mysql = match Self::env_or_panic("MYSQL_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("MYSQL_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
        };

        let credentials = match Self::env_or_panic("CREDENTIALS_SERVER_PORT") {
            None => None,
            Some(port) => {
                let port = port.parse::<u16>()?;
                Self::env_or_panic("CREDENTIALS_SERVER_HOSTNAME").map(|s| format!("{s}:{port}"))
            }
        };

        Ok(Self {
            postgres,
            mysql,
            credentials,
        })
    }

    pub fn postgres(&self) -> Option<&String> {
        self.postgres.as_ref()
    }

    pub fn mysql(&self) -> Option<&String> {
        self.mysql.as_ref()
    }

    pub fn credentials(&self) -> Option<&String> {
        self.credentials.as_ref()
    }

    fn env_or_panic(config_env: &str) -> Option<String> {
        match env::var(config_env) {
            Ok(r) => Some(r),
            Err(_) => {
                if env::var(SKIP_INTEGRATION_TESTS_ENV).is_ok_and(|s| s.to_lowercase() == "true") {
                    eprintln!("⚠️ WARNING: {SKIP_INTEGRATION_TESTS_ENV} is true, skipping integration tests for {config_env}");
                    return None;
                }
                panic!(
                    "

❌❌❌ ERROR ❌❌❌

Do one of:

CONFIGURE INTEGRATION TEST ENVIRONMENT

* follow testing/README.md and configure integration tests
* set env {config_env} to configure the service

- or -

SKIP INTEGRATION TESTS FOR \"{config_env}\"

* set env {SKIP_INTEGRATION_TESTS_ENV}=true
* unset env {config_env} 

❌❌❌ ERROR ❌❌❌

"
                );
            }
        }
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
