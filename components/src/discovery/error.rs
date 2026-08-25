use crate::secrets::SecretContextError;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin};
use switchgear_error::{IntoBoxedTrait, IntoContextError};
use switchgear_service_api::discovery::DiscoveryBackendStoreError;

pub struct DefaultDiscoveryBackendStoreError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<DefaultDiscoveryBackendStoreErrorSourceKind>>,
}

impl Debug for DefaultDiscoveryBackendStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultDiscoveryBackendStoreError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for DefaultDiscoveryBackendStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(
                f,
                "DefaultDiscoveryBackendStoreError: while {}: {}",
                self.context, s
            ),
            None => write!(f, "DefaultDiscoveryBackendStoreError: {}", self.context),
        }
    }
}

impl Error for DefaultDiscoveryBackendStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

#[derive(Debug)]
pub enum DefaultDiscoveryBackendStoreErrorSourceKind {
    Sqlx(Box<sqlx::Error>),
    Database(Box<sea_orm::DbErr>),
    Transaction(Box<sea_orm::TransactionError<sea_orm::DbErr>>),
    Deserialization(Box<reqwest::Error>),
    Http(Box<reqwest::Error>),
    Io(Box<std::io::Error>),
    JsonSerialization(Box<serde_json::Error>),
    UrlParse(Box<url::ParseError>),
    InvalidHeaderValue(Box<axum::http::header::InvalidHeaderValue>),
    Secp256k1(Box<secp256k1::Error>),
    ToStrError(Box<reqwest::header::ToStrError>),
    Secret(Box<dyn SecretContextError>),
}

impl Display for DefaultDiscoveryBackendStoreErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlx(e) => write!(f, "database error: {e}"),
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Transaction(e) => write!(f, "database transaction error: {e}"),
            Self::Deserialization(e) => write!(f, "deserialization failed: {e}"),
            Self::Http(e) => write!(f, "HTTP request failed: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::JsonSerialization(e) => write!(f, "JSON serialization error: {e}"),
            Self::UrlParse(e) => write!(f, "url parse error: {e}"),
            Self::InvalidHeaderValue(e) => write!(f, "invalid header value: {e}"),
            Self::Secp256k1(e) => write!(f, "secp256k1 error: {e}"),
            Self::ToStrError(e) => write!(f, "header value to str error: {e}"),
            Self::Secret(e) => Display::fmt(e, f),
        }
    }
}

impl Error for DefaultDiscoveryBackendStoreErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlx(e) => Some(&**e),
            Self::Database(e) => Some(&**e),
            Self::Transaction(e) => Some(&**e),
            Self::Deserialization(e) => Some(&**e),
            Self::Http(e) => Some(&**e),
            Self::Io(e) => Some(&**e),
            Self::JsonSerialization(e) => Some(&**e),
            Self::UrlParse(e) => Some(&**e),
            Self::InvalidHeaderValue(e) => Some(&**e),
            Self::Secp256k1(e) => Some(&**e),
            Self::ToStrError(e) => Some(&**e),
            Self::Secret(e) => Some(&**e),
        }
    }
}

impl DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: DefaultDiscoveryBackendStoreErrorSourceKind,
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

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source_kind(&self) -> Option<&DefaultDiscoveryBackendStoreErrorSourceKind> {
        self.source.as_deref()
    }
}

impl ContextError for DefaultDiscoveryBackendStoreError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(DefaultDiscoveryBackendStoreErrorSourceKind::Secret(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl DiscoveryBackendStoreError for DefaultDiscoveryBackendStoreError {}

impl IntoBoxedTrait<dyn DiscoveryBackendStoreError> for DefaultDiscoveryBackendStoreError {
    fn into_boxed(self) -> Box<dyn DiscoveryBackendStoreError> {
        Box::new(self)
    }
}

impl IntoContextError<sqlx::Error> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sqlx::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Sqlx(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<sea_orm::DbErr> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sea_orm::DbErr>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Database(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<sea_orm::TransactionError<sea_orm::DbErr>>
    for DefaultDiscoveryBackendStoreError
{
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sea_orm::TransactionError<sea_orm::DbErr>>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Transaction(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<serde_json::Error> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_json::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::JsonSerialization(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<reqwest::Error> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<reqwest::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let variant = if source.is_decode() {
            DefaultDiscoveryBackendStoreErrorSourceKind::Deserialization(source)
        } else {
            DefaultDiscoveryBackendStoreErrorSourceKind::Http(source)
        };
        Self::new(variant, origin.unwrap_or(ErrorOrigin::Upstream), message)
    }
}

impl IntoContextError<std::io::Error> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Io(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<url::ParseError> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<url::ParseError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::UrlParse(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<axum::http::header::InvalidHeaderValue>
    for DefaultDiscoveryBackendStoreError
{
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<axum::http::header::InvalidHeaderValue>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::InvalidHeaderValue(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<secp256k1::Error> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<secp256k1::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Secp256k1(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<reqwest::header::ToStrError> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<reqwest::header::ToStrError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::ToStrError(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<dyn SecretContextError> for DefaultDiscoveryBackendStoreError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn SecretContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(
            DefaultDiscoveryBackendStoreErrorSourceKind::Secret(source),
            effective,
            message,
        )
    }
}
