use crate::api::service::{HasServiceErrorSource, ServiceErrorSource};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Error, Debug)]
pub struct DiscoveryBackendStoreError {
    context: Cow<'static, str>,
    #[source]
    source: DiscoveryBackendStoreErrorSource,
    esource: ServiceErrorSource,
}

impl Display for DiscoveryBackendStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DiscoveryBackendStoreError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

#[derive(Error, Debug)]
pub enum DiscoveryBackendStoreErrorSource {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("deserialization failed: {0}")]
    Deserialization(reqwest::Error),
    #[error("HTTP request failed: {0}")]
    Http(reqwest::Error),
    #[error("HTTP status error: {0}")]
    HttpStatus(u16),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl DiscoveryBackendStoreError {
    pub fn new<C: Into<Cow<'static, str>>>(
        source: DiscoveryBackendStoreErrorSource,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source,
            esource,
        }
    }

    pub fn from_sqlx<C: Into<Cow<'static, str>>>(
        sqlx_error: sqlx::Error,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source: DiscoveryBackendStoreErrorSource::Sqlx(sqlx_error),
            esource,
        }
    }

    pub fn from_db<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        db_error: sea_orm::DbErr,
    ) -> Self {
        Self {
            source: DiscoveryBackendStoreErrorSource::Database(db_error),
            esource,
            context: context.into(),
        }
    }

    pub fn deserialization_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: reqwest::Error,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::Deserialization(original_error),
            esource,
            context,
        )
    }

    pub fn http_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: reqwest::Error,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::Http(original_error),
            esource,
            context,
        )
    }

    pub fn http_status_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        status_code: u16,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::HttpStatus(status_code),
            esource,
            context,
        )
    }

    pub fn io_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: std::io::Error,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::Io(original_error),
            esource,
            context,
        )
    }

    pub fn json_serialization_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: serde_json::Error,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::JsonSerialization(original_error),
            esource,
            context,
        )
    }

    pub fn internal_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        message: String,
    ) -> Self {
        Self::new(
            DiscoveryBackendStoreErrorSource::Internal(message),
            esource,
            context,
        )
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source(&self) -> &DiscoveryBackendStoreErrorSource {
        &self.source
    }

    pub fn esource(&self) -> ServiceErrorSource {
        self.esource
    }
}

impl HasServiceErrorSource for DiscoveryBackendStoreError {
    fn get_service_error_source(&self) -> ServiceErrorSource {
        self.esource
    }
}
