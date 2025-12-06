use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use switchgear_service_api::service::{HasServiceErrorSource, ServiceErrorSource};
use thiserror::Error;

#[derive(Error, Debug)]
pub struct OfferStoreError {
    context: Cow<'static, str>,
    #[source]
    source: OfferStoreErrorSourceKind,
    esource: ServiceErrorSource,
}

impl Display for OfferStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OfferStoreError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

#[derive(Error, Debug)]
pub enum OfferStoreErrorSourceKind {
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("resource not found")]
    NotFound,
    #[error("serialization failed: {0}")]
    Serialization(serde_json::Error),
    #[error("deserialization failed: {0}")]
    Deserialization(reqwest::Error),
    #[error("hash conversion failed")]
    HashConversion,
    #[error("HTTP request failed: {0}")]
    Http(reqwest::Error),
    #[error("HTTP status error: {0}")]
    HttpStatus(u16),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Invalid Input error: {0}")]
    InvalidInput(String),
}

impl OfferStoreError {
    fn new<C: Into<Cow<'static, str>>>(
        source: OfferStoreErrorSourceKind,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source,
            esource,
        }
    }

    // Convenience constructors for common error types
    pub fn not_found<C: Into<Cow<'static, str>>>(esource: ServiceErrorSource, context: C) -> Self {
        Self::new(OfferStoreErrorSourceKind::NotFound, esource, context)
    }

    pub fn serialization_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: serde_json::Error,
    ) -> Self {
        Self::new(
            OfferStoreErrorSourceKind::Serialization(original_error),
            esource,
            context,
        )
    }

    pub fn hash_conversion_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(OfferStoreErrorSourceKind::HashConversion, esource, context)
    }

    pub fn deserialization_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        original_error: reqwest::Error,
    ) -> Self {
        Self::new(
            OfferStoreErrorSourceKind::Deserialization(original_error),
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
            OfferStoreErrorSourceKind::Http(original_error),
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
            OfferStoreErrorSourceKind::HttpStatus(status_code),
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
            OfferStoreErrorSourceKind::Internal(message),
            esource,
            context,
        )
    }

    pub fn invalid_input_error<C: Into<Cow<'static, str>>>(context: C, message: String) -> Self {
        Self::new(
            OfferStoreErrorSourceKind::InvalidInput(message),
            ServiceErrorSource::Downstream,
            context,
        )
    }

    pub fn from_db<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        db_error: sea_orm::DbErr,
    ) -> Self {
        Self {
            source: OfferStoreErrorSourceKind::Database(db_error),
            esource,
            context: context.into(),
        }
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source(&self) -> &OfferStoreErrorSourceKind {
        &self.source
    }

    pub fn esource(&self) -> ServiceErrorSource {
        self.esource
    }
}

impl HasServiceErrorSource for OfferStoreError {
    fn get_service_error_source(&self) -> ServiceErrorSource {
        self.esource
    }
}
