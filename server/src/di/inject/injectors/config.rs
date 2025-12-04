use crate::config::ServerConfig;
use crate::ServiceEnablement;
use anyhow::{anyhow, Context};
use log::{info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ServiceEnablementInjector {
    lnurl_enabled: bool,
    discovery_enabled: bool,
    offer_enabled: bool,
}

impl ServiceEnablementInjector {
    pub fn new(enablement: Vec<ServiceEnablement>) -> Self {
        let start_all = enablement.is_empty() || enablement.contains(&ServiceEnablement::All);
        let lnurl_enabled = start_all || enablement.contains(&ServiceEnablement::LnUrl);
        let discovery_enabled = start_all || enablement.contains(&ServiceEnablement::Discovery);
        let offer_enabled = start_all || enablement.contains(&ServiceEnablement::Offer);

        Self {
            lnurl_enabled,
            discovery_enabled,
            offer_enabled,
        }
    }

    pub fn lnurl_enabled(&self) -> bool {
        self.lnurl_enabled
    }

    pub fn discovery_enabled(&self) -> bool {
        self.discovery_enabled
    }

    pub fn offer_enabled(&self) -> bool {
        self.offer_enabled
    }
}

#[derive(Clone, Debug)]
pub struct ServerConfigInjector {
    config: Arc<ServerConfig>,
    secrets: Arc<HashMap<String, String>>,
}

impl ServerConfigInjector {
    pub fn new(config_path: PathBuf) -> anyhow::Result<Self> {
        info!("loading configuration file: {config_path:?}");

        let config_content = std::fs::read_to_string(&config_path).with_context(|| {
            format!(
                "reading configuration file '{}'",
                config_path.to_string_lossy()
            )
        })?;

        let expanded_config = shellexpand::env(&config_content).with_context(|| {
            format!(
                "expanding configuration file env vars '{}'",
                config_path.to_string_lossy()
            )
        })?;

        let config: ServerConfig = serde_saphyr::from_str(&expanded_config).with_context(|| {
            format!(
                "parsing YAML configuration from file '{}'",
                config_path.to_string_lossy()
            )
        })?;

        info!("configuration loaded successfully: {config:?}");

        let secrets = match &config.secrets {
            None => {
                warn!("no secrets file in config, skipping secret loading");
                HashMap::new()
            }
            Some(secrets_path) => {
                let secrets_iter =
                    dotenvy::from_filename_iter(secrets_path.iter()).map_err(|_| {
                        anyhow!(
                            "error reading secrets file {}",
                            secrets_path.to_string_lossy()
                        )
                    })?;
                let mut secrets: HashMap<String, String> = HashMap::new();
                for secret in secrets_iter {
                    let (n, v) = secret.map_err(|_| {
                        anyhow!(
                            "error parsing secrets file {}",
                            secrets_path.to_string_lossy()
                        )
                    })?;
                    secrets.insert(format!("secret.{n}"), v);
                }
                info!(
                    "secrets loaded successfully from: {}",
                    secrets_path.to_string_lossy()
                );
                secrets
            }
        };

        Ok(Self {
            config: Arc::new(config),
            secrets: Arc::new(secrets),
        })
    }

    pub fn get(&self) -> &ServerConfig {
        self.config.as_ref()
    }

    pub fn secrets(&self) -> &HashMap<String, String> {
        self.secrets.as_ref()
    }
}
