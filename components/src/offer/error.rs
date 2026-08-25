use crate::secrets::SecretContextError;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin};
use switchgear_error::{IntoBoxedTrait, IntoContextError};
use switchgear_service_api::offer::OfferStoreError;

pub struct DefaultOfferStoreError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<DefaultOfferStoreErrorSourceKind>>,
}

impl Debug for DefaultOfferStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultOfferStoreError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for DefaultOfferStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(f, "DefaultOfferStoreError: while {}: {}", self.context, s),
            None => write!(f, "DefaultOfferStoreError: {}", self.context),
        }
    }
}

impl Error for DefaultOfferStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

#[derive(Debug)]
pub enum DefaultOfferStoreErrorSourceKind {
    Sqlx(Box<sqlx::Error>),
    Database(Box<sea_orm::DbErr>),
    Serialization(Box<serde_json::Error>),
    Deserialization(Box<reqwest::Error>),
    Http(Box<reqwest::Error>),
    UrlParse(Box<url::ParseError>),
    InvalidHeaderValue(Box<axum::http::header::InvalidHeaderValue>),
    InvalidInput(String),
    Secret(Box<dyn SecretContextError>),
}

impl Display for DefaultOfferStoreErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlx(e) => write!(f, "database error: {e}"),
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Serialization(e) => write!(f, "serialization failed: {e}"),
            Self::Deserialization(e) => write!(f, "deserialization failed: {e}"),
            Self::Http(e) => write!(f, "HTTP request failed: {e}"),
            Self::UrlParse(e) => write!(f, "url parse error: {e}"),
            Self::InvalidHeaderValue(e) => write!(f, "invalid header value: {e}"),
            Self::InvalidInput(msg) => write!(f, "Invalid Input error: {msg}"),
            Self::Secret(e) => Display::fmt(e, f),
        }
    }
}

impl Error for DefaultOfferStoreErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlx(e) => Some(&**e),
            Self::Database(e) => Some(&**e),
            Self::Serialization(e) => Some(&**e),
            Self::Deserialization(e) => Some(&**e),
            Self::Http(e) => Some(&**e),
            Self::UrlParse(e) => Some(&**e),
            Self::InvalidHeaderValue(e) => Some(&**e),
            Self::InvalidInput(_) => None,
            Self::Secret(e) => Some(&**e),
        }
    }
}

impl DefaultOfferStoreError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: DefaultOfferStoreErrorSourceKind,
        origin: ErrorOrigin,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            origin,
            location: Location::caller(),
            source: Some(Box::new(source)),
        }
    }

    #[track_caller]
    pub(crate) fn message<C: Into<Cow<'static, str>>>(origin: ErrorOrigin, context: C) -> Self {
        Self {
            context: context.into(),
            origin,
            location: Location::caller(),
            source: None,
        }
    }

    #[track_caller]
    pub(crate) fn invalid_input_error<C: Into<Cow<'static, str>>>(
        context: C,
        message: String,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::InvalidInput(message),
            ErrorOrigin::Downstream,
            context,
        )
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source_kind(&self) -> Option<&DefaultOfferStoreErrorSourceKind> {
        self.source.as_deref()
    }
}

impl ContextError for DefaultOfferStoreError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(DefaultOfferStoreErrorSourceKind::Secret(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl OfferStoreError for DefaultOfferStoreError {}

impl IntoBoxedTrait<dyn OfferStoreError> for DefaultOfferStoreError {
    fn into_boxed(self) -> Box<dyn OfferStoreError> {
        Box::new(self)
    }
}

impl IntoContextError<sqlx::Error> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sqlx::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::Sqlx(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<sea_orm::DbErr> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sea_orm::DbErr>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::Database(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<serde_json::Error> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_json::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::Serialization(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<reqwest::Error> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<reqwest::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let variant = if source.is_decode() {
            DefaultOfferStoreErrorSourceKind::Deserialization(source)
        } else {
            DefaultOfferStoreErrorSourceKind::Http(source)
        };
        Self::new(variant, origin.unwrap_or(ErrorOrigin::Upstream), message)
    }
}

impl IntoContextError<url::ParseError> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<url::ParseError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::UrlParse(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<axum::http::header::InvalidHeaderValue> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<axum::http::header::InvalidHeaderValue>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultOfferStoreErrorSourceKind::InvalidHeaderValue(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<dyn SecretContextError> for DefaultOfferStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn SecretContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(
            DefaultOfferStoreErrorSourceKind::Secret(source),
            effective,
            message,
        )
    }
}
