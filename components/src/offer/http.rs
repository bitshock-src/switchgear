use crate::offer::error::DefaultOfferStoreError;
use crate::secrets::SecretStore;
use async_trait::async_trait;
use reqwest::{Certificate, Client, ClientBuilder, IntoUrl, StatusCode};
use rustls::pki_types::CertificateDer;
use secrecy::{ExposeSecret, SecretString};
use std::time::Duration;
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::offer::{
    HttpOfferClient, OfferMetadata, OfferMetadataStore, OfferRecord, OfferStore,
};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct HttpOfferStore {
    client: Client,
    secret_store: SecretStore,
    bearer_secret: String,
    offer_url: String,
    metadata_url: String,
    health_check_url: String,
}

impl HttpOfferStore {
    #[tracing::instrument(skip_all)]
    pub fn create<U: IntoUrl>(
        base_url: U,
        total_timeout: Duration,
        connect_timeout: Duration,
        trusted_roots: &[CertificateDer],
        secret_store: SecretStore,
        bearer_secret: impl Into<String>,
    ) -> Result<Self, DefaultOfferStoreError> {
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

    #[tracing::instrument(skip_all)]
    fn with_client<U: IntoUrl>(
        client: Client,
        base_url: U,
        secret_store: SecretStore,
        bearer_secret: String,
    ) -> Result<Self, DefaultOfferStoreError> {
        let base_url = base_url.as_str().trim_end_matches('/').to_string();

        let offer_url = format!("{base_url}/offers");
        Url::parse(&offer_url).with_foreign_context(
            || format!("parsing service url {offer_url}"),
            ErrorOrigin::Upstream,
        )?;

        let metadata_url = format!("{base_url}/metadata");
        Url::parse(&metadata_url).with_foreign_context(
            || format!("parsing service url {metadata_url}"),
            ErrorOrigin::Upstream,
        )?;

        let health_check_url = format!("{base_url}/health");
        Url::parse(&health_check_url).with_foreign_context(
            || format!("parsing service url {health_check_url}"),
            ErrorOrigin::Upstream,
        )?;

        Ok(Self {
            client,
            secret_store,
            bearer_secret,
            offer_url,
            metadata_url,
            health_check_url,
        })
    }

    fn offers_partition_url(&self, partition: &str) -> String {
        format!("{}/{}", self.offer_url, partition)
    }

    fn offers_partition_id_url(&self, partition: &str, id: &Uuid) -> String {
        format!("{}/{}", self.offers_partition_url(partition), id)
    }

    fn metadata_partition_url(&self, partition: &str) -> String {
        format!("{}/{}", self.metadata_url, partition)
    }

    fn metadata_partition_id_url(&self, partition: &str, id: &Uuid) -> String {
        format!("{}/{}", self.metadata_partition_url(partition), id)
    }

    fn bearer(&self) -> Result<SecretString, DefaultOfferStoreError> {
        self.secret_store
            .get_str(&self.bearer_secret)
            .with_chained_context(
                || format!("resolving http offer store bearer {}", self.bearer_secret),
                ErrorOrigin::Internal,
            )
    }

    #[tracing::instrument(skip_all)]
    fn general_error(status: StatusCode, context: &str) -> DefaultOfferStoreError {
        if status.is_success() {
            return DefaultOfferStoreError::message(
                ErrorOrigin::Upstream,
                format!("{context}: unexpected http status {status}"),
            );
        }
        if status.is_client_error() {
            return DefaultOfferStoreError::invalid_input_error(
                context.to_string(),
                format!("invalid input, http status: {status}"),
            );
        }
        DefaultOfferStoreError::message(
            ErrorOrigin::Upstream,
            format!("http status {}: {context}", status.as_u16()),
        )
    }
}

#[async_trait]
impl OfferStore for HttpOfferStore {
    type Error = DefaultOfferStoreError;

    #[tracing::instrument(skip_all)]
    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
        sparse: Option<bool>,
    ) -> Result<Option<OfferRecord>, Self::Error> {
        let sparse = sparse.unwrap_or(true);
        let url = self.offers_partition_id_url(partition, id);
        let url = format!("{url}?sparse={sparse}");
        let token = self.bearer()?;
        let response = self
            .client
            .get(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(|| format!("get offer {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::OK => {
                let offer = response.json::<OfferRecord>().await.with_foreign_context(
                    || format!("parsing offer {id}"),
                    ErrorOrigin::Upstream,
                )?;
                Ok(Some(offer))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(status, &format!("get offer {url}"))),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferRecord>, Self::Error> {
        let url = self.offers_partition_url(partition);
        let url = format!("{url}?start={start}&count={count}");
        let token = self.bearer()?;
        let response = self
            .client
            .get(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(|| format!("get all offers {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::OK => {
                let offer_records = response
                    .json::<Vec<OfferRecord>>()
                    .await
                    .with_foreign_context(
                        || format!("parsing all offers for {url}"),
                        ErrorOrigin::Upstream,
                    )?;
                Ok(offer_records)
            }
            status => Err(Self::general_error(
                status,
                &format!("get all offers {url}"),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error> {
        let token = self.bearer()?;
        let response = self
            .client
            .post(&self.offer_url)
            .bearer_auth(token.expose_secret())
            .json(&offer)
            .send()
            .await
            .with_foreign_context(
                || format!("post offer: {}, url: {}", offer.id, self.offer_url),
                ErrorOrigin::Upstream,
            )?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(offer.id)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!("post offer: {}, url: {}", offer.id, self.offer_url),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put_offer(&self, offer: OfferRecord) -> Result<bool, Self::Error> {
        let url = self.offers_partition_id_url(&offer.partition, &offer.id);
        let token = self.bearer()?;
        let response = self
            .client
            .put(&url)
            .bearer_auth(token.expose_secret())
            .json(&offer)
            .send()
            .await
            .with_foreign_context(|| format!("put offer {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::CREATED => Ok(true),
            StatusCode::NO_CONTENT => Ok(false),
            status => Err(Self::general_error(status, &format!("put offer {url}"))),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let url = self.offers_partition_id_url(partition, id);
        let token = self.bearer()?;
        let response = self
            .client
            .delete(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(|| format!("delete offer {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(status, &format!("delete offer {url}"))),
        }
    }
}

#[async_trait]
impl OfferMetadataStore for HttpOfferStore {
    type Error = DefaultOfferStoreError;

    #[tracing::instrument(skip_all)]
    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error> {
        let url = self.metadata_partition_id_url(partition, id);
        let token = self.bearer()?;
        let response = self
            .client
            .get(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(
                || format!("get offer metadata {url}"),
                ErrorOrigin::Upstream,
            )?;

        match response.status() {
            StatusCode::OK => {
                let metadata = response
                    .json::<OfferMetadata>()
                    .await
                    .with_foreign_context(
                        || format!("parse offer metadata {url}"),
                        ErrorOrigin::Upstream,
                    )?;
                Ok(Some(metadata))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!("get offer metadata {url}"),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error> {
        let url = self.metadata_partition_url(partition);
        let url = format!("{url}?start={start}&count={count}");
        let token = self.bearer()?;
        let response = self
            .client
            .get(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(|| format!("get all metadata {url}"), ErrorOrigin::Upstream)?;

        match response.status() {
            StatusCode::OK => {
                let metadata_all = response
                    .json::<Vec<OfferMetadata>>()
                    .await
                    .with_foreign_context(
                        || format!("parse all metadata {url}"),
                        ErrorOrigin::Upstream,
                    )?;
                Ok(metadata_all)
            }
            status => Err(Self::general_error(
                status,
                &format!("get all metadata {url}"),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn post_metadata(&self, metadata: OfferMetadata) -> Result<Option<Uuid>, Self::Error> {
        let token = self.bearer()?;
        let response = self
            .client
            .post(&self.metadata_url)
            .bearer_auth(token.expose_secret())
            .json(&metadata)
            .send()
            .await
            .with_foreign_context(
                || {
                    format!(
                        "post offer metadata {}, url: {}",
                        metadata.id, self.metadata_url
                    )
                },
                ErrorOrigin::Upstream,
            )?;

        match response.status() {
            StatusCode::CREATED => Ok(Some(metadata.id)),
            StatusCode::CONFLICT => Ok(None),
            status => Err(Self::general_error(
                status,
                &format!(
                    "post offer metadata {}, url: {}",
                    metadata.id, self.metadata_url
                ),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn put_metadata(&self, metadata: OfferMetadata) -> Result<bool, Self::Error> {
        let url = self.metadata_partition_id_url(&metadata.partition, &metadata.id);
        let token = self.bearer()?;
        let response = self
            .client
            .put(&url)
            .bearer_auth(token.expose_secret())
            .json(&metadata)
            .send()
            .await
            .with_foreign_context(
                || format!("put offer metadata {url}"),
                ErrorOrigin::Upstream,
            )?;

        match response.status() {
            StatusCode::CREATED => Ok(true),
            StatusCode::NO_CONTENT => Ok(false),
            status => Err(Self::general_error(
                status,
                &format!("put offer metadata {url}"),
            )),
        }
    }

    #[tracing::instrument(skip_all)]
    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error> {
        let url = self.metadata_partition_id_url(partition, id);
        let token = self.bearer()?;
        let response = self
            .client
            .delete(&url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .with_foreign_context(
                || format!("delete offer metadata {url}"),
                ErrorOrigin::Upstream,
            )?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => Err(Self::general_error(
                status,
                &format!("delete offer metadata {url}"),
            )),
        }
    }
}

#[async_trait]
impl HttpOfferClient for HttpOfferStore {
    #[tracing::instrument(skip_all)]
    async fn health(&self) -> Result<(), <Self as OfferStore>::Error> {
        let token = self.bearer()?;
        let response = self
            .client
            .get(&self.health_check_url)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .foreign_context("health check", ErrorOrigin::Upstream)?;
        if !response.status().is_success() {
            return Err(DefaultOfferStoreError::message(
                ErrorOrigin::Upstream,
                format!("http status {}: health check", response.status().as_u16()),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::offer::http::HttpOfferStore;
    use crate::secrets::{SecretStore, SecretStoreConfig, SecretStoreSecretConfig};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use url::Url;
    use uuid::Uuid;

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
        let client = HttpOfferStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://offers-base.com").unwrap(),
            empty_secret_store(),
            "TOKEN".to_owned(),
        )
        .unwrap();

        assert_eq!(&client.offer_url, "https://offers-base.com/offers");
        assert_eq!(&client.metadata_url, "https://offers-base.com/metadata");

        let client = HttpOfferStore::with_client(
            reqwest::Client::default(),
            Url::parse("https://offers-base.com/").unwrap(),
            empty_secret_store(),
            "TOKEN".to_owned(),
        )
        .unwrap();

        assert_eq!(&client.offer_url, "https://offers-base.com/offers");
        assert_eq!(&client.metadata_url, "https://offers-base.com/metadata");

        assert_eq!(&client.health_check_url, "https://offers-base.com/health");

        let offers_partition_url = client.offers_partition_url("partition");
        assert_eq!(
            "https://offers-base.com/offers/partition",
            offers_partition_url,
        );

        let id = Uuid::new_v4();
        let offers_partition_id_url = client.offers_partition_id_url("partition", &id);
        assert_eq!(
            format!("https://offers-base.com/offers/partition/{id}"),
            offers_partition_id_url,
        );

        let metadata_partition_url = client.metadata_partition_url("partition");
        assert_eq!(
            "https://offers-base.com/metadata/partition",
            metadata_partition_url,
        );

        let id = Uuid::new_v4();
        let metadata_partition_id_url = client.metadata_partition_id_url("partition", &id);
        assert_eq!(
            format!("https://offers-base.com/metadata/partition/{id}"),
            metadata_partition_id_url,
        );
    }
}
