use crate::discovery::error::DefaultDiscoveryBackendStoreError;
use crate::metrics::http::record_http_request;
use crate::secrets::SecretStore;
use async_trait::async_trait;
use reqwest::{Certificate, Client, ClientBuilder, IntoUrl, StatusCode};
use rustls::pki_types::CertificateDer;
use secp256k1::PublicKey;
use secrecy::{ExposeSecret, SecretString};
use std::time::{Duration, Instant};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendStore, DiscoveryBackends,
    HttpDiscoveryBackendClient,
};
use url::Url;

#[derive(Clone, Debug)]
pub struct HttpDiscoveryBackendStore {
    client: Client,
    secret_store: SecretStore,
    bearer_secret: String,
    discovery_url: String,
    health_check_url: String,
    server_address: String,
    server_port: u16,
}

impl HttpDiscoveryBackendStore {
    pub fn create<U: IntoUrl>(
        base_url: U,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: &[CertificateDer],
        secret_store: SecretStore,
        bearer_secret: impl Into<String>,
    ) -> Result<Self, DefaultDiscoveryBackendStoreError> {
        let mut builder = ClientBuilder::new();

        for root in trusted_roots {
            let root = Certificate::from_der(root).with_foreign_context(
                || format!("parsing certificate for url: {}", base_url.as_str()),
                ErrorOrigin::Internal,
            )?;
            builder = builder.add_root_certificate(root);
        }

        let client = builder
            .use_rustls_tls()
            .timeout(total_timeout)
            .connect_timeout(connect_timeout)
            .build()
            .with_foreign_context(
                || format!("creating http client with base url: {}", base_url.as_str()),
                ErrorOrigin::Internal,
            )?;
        Self::with_client(client, base_url, secret_store, bearer_secret.into())
    }

    pub fn with_client<U: IntoUrl>(
        client: Client,
        base_url: U,
        secret_store: SecretStore,
        bearer_secret: String,
    ) -> Result<Self, DefaultDiscoveryBackendStoreError> {
        let base_url = base_url.as_str().trim_end_matches('/').to_string();
        let discovery_url = format!("{base_url}/discovery");
        let parsed_discovery_url = Url::parse(&discovery_url).with_foreign_context(
            || format!("parsing service url {discovery_url}"),
            ErrorOrigin::Upstream,
        )?;
        let server_address = parsed_discovery_url
            .host_str()
            .unwrap_or_default()
            .to_owned();
        let server_port = parsed_discovery_url.port_or_known_default().unwrap_or(0);

        let health_check_url = format!("{base_url}/health");
        Url::parse(&health_check_url).with_foreign_context(
            || format!("parsing service url {health_check_url}"),
            ErrorOrigin::Upstream,
        )?;

        Ok(Self {
            client,
            secret_store,
            bearer_secret,
            discovery_url,
            health_check_url,
            server_address,
            server_port,
        })
    }

    fn discovery_public_key_url(&self, public_key: &PublicKey) -> String {
        format!("{}/{}", self.discovery_url, public_key)
    }

    fn bearer(&self) -> Result<SecretString, DefaultDiscoveryBackendStoreError> {
        self.secret_store
            .get_str(&self.bearer_secret)
            .with_chained_context(
                || {
                    format!(
                        "resolving http discovery store bearer {}",
                        self.bearer_secret
                    )
                },
                ErrorOrigin::Internal,
            )
    }

    fn general_error(status: StatusCode, context: &str) -> DefaultDiscoveryBackendStoreError {
        if status.is_success() {
            return DefaultDiscoveryBackendStoreError::message(
                ErrorOrigin::Upstream,
                format!("{context}: unexpected http status {status}"),
            );
        }
        if status.is_client_error() {
            return DefaultDiscoveryBackendStoreError::message(
                ErrorOrigin::Downstream,
                format!("{context}: invalid input, http status: {status}"),
            );
        }
        DefaultDiscoveryBackendStoreError::message(
            ErrorOrigin::Upstream,
            format!("http status {}: {context}", status.as_u16()),
        )
    }
}

#[async_trait]
impl DiscoveryBackendStore for HttpDiscoveryBackendStore {
    type Error = DefaultDiscoveryBackendStoreError;

    #[tracing::instrument(skip_all)]
    async fn get(&self, public_key: &PublicKey) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let url = self.discovery_public_key_url(public_key);

        let token = self.bearer()?;

        let started = Instant::now();
        let response = self
            .client
            .get(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await;
        record_http_request(
            started.elapsed(),
            "GET",
            "/discovery/{public_key}",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("get backend {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::OK => {
                let backend: DiscoveryBackend = response.json().await.with_foreign_context(
                    || format!("parse backend {url}"),
                    ErrorOrigin::Upstream,
                )?;
                Ok(Some(backend))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(status, &format!("get backend {url}"))),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_all(&self, requested_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let url = &self.discovery_url;
        let token = self.bearer()?;
        let client = self.client.get(url).bearer_auth(token.expose_secret());

        let client = if let Some(requested_etag) = requested_etag {
            client.header(
                reqwest::header::IF_NONE_MATCH,
                hex::encode(requested_etag.to_be_bytes()),
            )
        } else {
            client
        };

        let started = Instant::now();
        let response = client.send().await;
        record_http_request(
            started.elapsed(),
            "GET",
            "/discovery",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("get all backends {url}"), ErrorOrigin::Upstream)?;

        let response_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .ok_or_else(|| {
                DefaultDiscoveryBackendStoreError::message(
                    ErrorOrigin::Upstream,
                    format!(
                        "parsing etag header response from get all backends {url}: missing expected etag"
                    ),
                )
            })?
            .to_str()
            .with_foreign_context(
                || format!("parsing etag header response from get all backends {url}"),
                ErrorOrigin::Upstream,
            )?;

        let response_etag = DiscoveryBackends::etag_from_str(response_etag).with_foreign_context(
            || {
                format!(
                    "parsing etag '{response_etag}' header response from get all backends {url}"
                )
            },
            ErrorOrigin::Upstream,
        )?;

        match response.status() {
            StatusCode::OK => {
                let backends: Vec<DiscoveryBackend> = response.json().await.with_foreign_context(
                    || format!("parse all backends {url}"),
                    ErrorOrigin::Upstream,
                )?;

                Ok(DiscoveryBackends {
                    etag: response_etag,
                    backends: Some(backends),
                })
            }
            StatusCode::NOT_MODIFIED => Ok(DiscoveryBackends {
                etag: response_etag,
                backends: None,
            }),
            status => Err(Self::general_error(
                status,
                &format!("get all backends {url}"),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn post(&self, backend: DiscoveryBackend) -> Result<Option<PublicKey>, Self::Error> {
        let token = self.bearer()?;

        let started = Instant::now();
        let response = self
            .client
            .post(&self.discovery_url)
            .bearer_auth(token.expose_secret())
            .json(&backend)
            .send()
            .await;
        record_http_request(
            started.elapsed(),
            "POST",
            "/discovery",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response.with_foreign_context(
            || {
                format!(
                    "post backend: {}, url: {}",
                    backend.public_key, self.discovery_url
                )
            },
            ErrorOrigin::Upstream,
        )?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(backend.public_key)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!(
                    "post backend: {}, url: {}",
                    backend.public_key, self.discovery_url
                ),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let url = self.discovery_public_key_url(&backend.public_key);

        let token = self.bearer()?;

        let started = Instant::now();
        let response = self
            .client
            .put(&url)
            .bearer_auth(token.expose_secret())
            .json(&backend.backend)
            .send()
            .await;
        record_http_request(
            started.elapsed(),
            "PUT",
            "/discovery/{public_key}",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("put backend {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(false),
            StatusCode::CREATED => Ok(true),
            status => Err(Self::general_error(status, &format!("put backend {url}"))),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn patch(&self, backend: DiscoveryBackendPatch) -> Result<bool, Self::Error> {
        let url = self.discovery_public_key_url(&backend.public_key);

        let token = self.bearer()?;

        let started = Instant::now();
        let response = self
            .client
            .patch(&url)
            .bearer_auth(token.expose_secret())
            .json(&backend.backend)
            .send()
            .await;
        record_http_request(
            started.elapsed(),
            "PATCH",
            "/discovery/{public_key}",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("patch backend {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(status, &format!("patch backend {url}"))),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn delete(&self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let url = self.discovery_public_key_url(public_key);

        let token = self.bearer()?;

        let started = Instant::now();
        let response = self
            .client
            .delete(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await;
        record_http_request(
            started.elapsed(),
            "DELETE",
            "/discovery/{public_key}",
            &self.server_address,
            self.server_port,
            &response,
        );
        let response = response
            .with_foreign_context(|| format!("delete backend {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(
                status,
                &format!("delete backend: {url}"),
            )),
        }
    }
}

#[async_trait]
impl HttpDiscoveryBackendClient for HttpDiscoveryBackendStore {
    #[tracing::instrument(skip_all)]
    async fn health(&self) -> Result<(), Self::Error> {
        let token = self.bearer()?;
        let response = self
            .client
            .get(&self.health_check_url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .foreign_context("health check", ErrorOrigin::Upstream)?;
        if !response.status().is_success() {
            return Err(DefaultDiscoveryBackendStoreError::message(
                ErrorOrigin::Upstream,
                format!("http status {}: health check", response.status().as_u16()),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::discovery::http::HttpDiscoveryBackendStore;
    use crate::secrets::{SecretStore, SecretStoreConfig, SecretStoreSecretConfig};
    use anyhow::anyhow;
    use rand::Rng;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use url::Url;

    fn empty_secret_store() -> SecretStore {
        SecretStore::create(&SecretStoreConfig {
            ttl: 60.0,
            secrets: BTreeMap::from([(
                "TOKEN".to_owned(),
                SecretStoreSecretConfig {
                    path: PathBuf::from("/dev/null"),
                },
            )]),
        })
    }

    #[test]
    fn base_urls() {
        let _ = rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com").unwrap(),
            empty_secret_store(),
            "TOKEN".to_owned(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com/").unwrap(),
            empty_secret_store(),
            "TOKEN".to_owned(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        assert_eq!(&client.health_check_url, "https://base.com/health");

        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        let secret_key = SecretKey::from_byte_array(rng.r#gen::<[u8; 32]>()).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let discovery_partition_public_key_url = client.discovery_public_key_url(&public_key);
        assert_eq!(
            format!("https://base.com/discovery/{public_key}"),
            discovery_partition_public_key_url,
        );
    }
}
