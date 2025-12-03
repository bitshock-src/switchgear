use crate::api::discovery::{
    DiscoveryBackend, DiscoveryBackendAddress, DiscoveryBackendPatch, DiscoveryBackendStore,
    DiscoveryBackends, HttpDiscoveryBackendClient,
};
use crate::api::service::ServiceErrorSource;
use crate::components::discovery::error::DiscoveryBackendStoreError;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Certificate, Client, ClientBuilder, IntoUrl, StatusCode};
use rustls::pki_types::CertificateDer;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
pub struct HttpDiscoveryBackendStore {
    client: Client,
    discovery_url: String,
    health_check_url: String,
}

impl HttpDiscoveryBackendStore {
    pub fn create<U: IntoUrl>(
        base_url: U,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: &[CertificateDer],
        authorization: String,
    ) -> Result<Self, DiscoveryBackendStoreError> {
        let mut headers = HeaderMap::new();
        let mut auth_value =
            HeaderValue::from_str(&format!("Bearer {authorization}")).map_err(|e| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Internal,
                    format!("creating http client with base url: {}", base_url.as_str()),
                    e.to_string(),
                )
            })?;
        auth_value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);

        let mut builder = ClientBuilder::new();

        for root in trusted_roots {
            let root = Certificate::from_der(root).map_err(|e| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Internal,
                    format!("parsing certificate for url: {}", base_url.as_str()),
                    e.to_string(),
                )
            })?;
            builder = builder.add_root_certificate(root);
        }

        let client = builder
            .default_headers(headers)
            .use_rustls_tls()
            .timeout(total_timeout)
            .connect_timeout(connect_timeout)
            .build()
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Internal,
                    format!("creating http client with base url: {}", base_url.as_str()),
                    e,
                )
            })?;
        Self::with_client(client, base_url)
    }

    pub fn with_client<U: IntoUrl>(
        client: Client,
        base_url: U,
    ) -> Result<Self, DiscoveryBackendStoreError> {
        let base_url = base_url.as_str().trim_end_matches('/').to_string();
        let discovery_url = format!("{base_url}/discovery");
        Url::parse(&discovery_url).map_err(|e| {
            DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {discovery_url}"),
                e.to_string(),
            )
        })?;

        let health_check_url = format!("{base_url}/health");
        Url::parse(&health_check_url).map_err(|e| {
            DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!("parsing service url {health_check_url}"),
                e.to_string(),
            )
        })?;

        Ok(Self {
            client,
            discovery_url,
            health_check_url,
        })
    }

    fn discovery_address_url(&self, addr: &DiscoveryBackendAddress) -> String {
        format!("{}/{}", self.discovery_url, addr.encoded())
    }

    fn general_error(status: StatusCode, context: &str) -> DiscoveryBackendStoreError {
        if status.is_success() {
            return DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                context.to_string(),
                format!("unexpected http status {status}"),
            );
        }
        if status.is_client_error() {
            return DiscoveryBackendStoreError::invalid_input_error(
                context.to_string(),
                format!("invalid input, http status: {status}"),
            );
        }
        DiscoveryBackendStoreError::http_status_error(
            ServiceErrorSource::Upstream,
            context.to_string(),
            status.as_u16(),
        )
    }
}

#[async_trait]
impl DiscoveryBackendStore for HttpDiscoveryBackendStore {
    type Error = DiscoveryBackendStoreError;

    async fn get(
        &self,
        addr: &DiscoveryBackendAddress,
    ) -> Result<Option<DiscoveryBackend>, Self::Error> {
        let url = self.discovery_address_url(addr);

        let response = self.client.get(&url).send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("get backend {url}"),
                e,
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let backend: DiscoveryBackend = response.json().await.map_err(|e| {
                    DiscoveryBackendStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parse backend {url}"),
                        e,
                    )
                })?;
                Ok(Some(backend))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(status, &format!("get backend {url}"))),
        }
    }

    async fn get_all(&self, requested_etag: Option<u64>) -> Result<DiscoveryBackends, Self::Error> {
        let url = &self.discovery_url;
        let client = self.client.get(url);
        let client = if let Some(requested_etag) = requested_etag {
            client.header(
                reqwest::header::IF_NONE_MATCH,
                hex::encode(requested_etag.to_be_bytes()),
            )
        } else {
            client
        };
        let response = client.send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("get all backends {url}"),
                e,
            )
        })?;

        let response_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .ok_or_else(|| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Upstream,
                    format!("parsing etag header response from get all backends {url}"),
                    "missing expected etag".to_string(),
                )
            })?
            .to_str()
            .map_err(|e| {
                DiscoveryBackendStoreError::internal_error(
                    ServiceErrorSource::Upstream,
                    format!("parsing etag header response from get all backends {url}"),
                    e.to_string(),
                )
            })?;

        let response_etag = DiscoveryBackends::etag_from_str(response_etag).map_err(|e| {
            DiscoveryBackendStoreError::internal_error(
                ServiceErrorSource::Upstream,
                format!(
                    "parsing etag '{response_etag}' header response from get all backends {url}"
                ),
                e.to_string(),
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let backends: Vec<DiscoveryBackend> = response.json().await.map_err(|e| {
                    DiscoveryBackendStoreError::deserialization_error(
                        ServiceErrorSource::Upstream,
                        format!("parse all backends {url}"),
                        e,
                    )
                })?;

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

    async fn post(
        &self,
        backend: DiscoveryBackend,
    ) -> Result<Option<DiscoveryBackendAddress>, Self::Error> {
        let response = self
            .client
            .post(&self.discovery_url)
            .json(&backend)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!(
                        "post backend: {}, url: {}",
                        backend.address, &self.discovery_url
                    ),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(backend.address)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!(
                    "post backend: {}, url: {}",
                    backend.address, &self.discovery_url
                ),
            )),
        }
    }

    async fn put(&self, backend: DiscoveryBackend) -> Result<bool, Self::Error> {
        let url = self.discovery_address_url(&backend.address);

        let response = self
            .client
            .put(&url)
            .json(&backend.backend)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("put backend {url}"),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(false),
            StatusCode::CREATED => Ok(true),
            status => Err(Self::general_error(status, &format!("put backend {url}"))),
        }
    }

    async fn patch(&self, backend: DiscoveryBackendPatch) -> Result<bool, Self::Error> {
        let url = self.discovery_address_url(&backend.address);

        let response = self
            .client
            .patch(&url)
            .json(&backend.backend)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    format!("patch backend {url}"),
                    e,
                )
            })?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(status, &format!("patch backend {url}"))),
        }
    }

    async fn delete(&self, addr: &DiscoveryBackendAddress) -> Result<bool, Self::Error> {
        let url = self.discovery_address_url(addr);

        let response = self.client.delete(&url).send().await.map_err(|e| {
            DiscoveryBackendStoreError::http_error(
                ServiceErrorSource::Upstream,
                format!("delete backend {url}"),
                e,
            )
        })?;

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
    async fn health(&self) -> Result<(), Self::Error> {
        let response = self
            .client
            .get(&self.health_check_url)
            .send()
            .await
            .map_err(|e| {
                DiscoveryBackendStoreError::http_error(
                    ServiceErrorSource::Upstream,
                    "health check",
                    e,
                )
            })?;
        if !response.status().is_success() {
            return Err(DiscoveryBackendStoreError::http_status_error(
                ServiceErrorSource::Upstream,
                "health check",
                response.status().as_u16(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::api::discovery::DiscoveryBackendAddress;
    use crate::components::discovery::http::HttpDiscoveryBackendStore;
    use anyhow::anyhow;
    use url::Url;

    #[test]
    fn base_urls() {
        let _ = rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        let client = HttpDiscoveryBackendStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://base.com/").unwrap(),
        )
        .unwrap();

        assert_eq!(&client.discovery_url, "https://base.com/discovery");

        assert_eq!(&client.health_check_url, "https://base.com/health");

        let addr = DiscoveryBackendAddress::Url("https://remote.com/backend".parse().unwrap());
        let discovery_partition_address_url = client.discovery_address_url(&addr);
        assert_eq!(
            format!("https://base.com/discovery/{}", addr.encoded()),
            discovery_partition_address_url,
        );
    }
}
