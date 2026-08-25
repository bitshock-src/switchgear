use axum::extract::rejection::JsonRejection;
use axum::http::HeaderValue;
use axum::http::header::{InvalidHeaderValue, ToStrError};
use axum::response::{IntoResponse, Response};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoContextError};
use switchgear_service_api::discovery::DiscoveryBackendStoreError;

use crate::axum::crud::error::CrudError;

pub struct DiscoveryCrudError(CrudError);

impl DiscoveryCrudError {
    #[track_caller]
    pub fn not_found() -> Self {
        Self(CrudError::not_found())
    }

    #[track_caller]
    pub fn conflict(location: HeaderValue) -> Self {
        Self(CrudError::conflict(location))
    }
}

impl IntoContextError<JsonRejection> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<JsonRejection>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_json_rejection(
            source,
            message.into(),
            origin,
        ))
    }
}

impl Debug for DiscoveryCrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for DiscoveryCrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Error for DiscoveryCrudError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl ContextError for DiscoveryCrudError {
    fn origin(&self) -> ErrorOrigin {
        self.0.origin()
    }

    fn location(&self) -> &'static Location<'static> {
        self.0.location()
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        self.0.source_context()
    }
}

impl IntoContextError<dyn DiscoveryBackendStoreError> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiscoveryBackendStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_discovery_backend_store(
            source,
            message.into(),
            origin,
        ))
    }
}

impl IntoContextError<InvalidHeaderValue> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<InvalidHeaderValue>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_invalid_header_value(
            source,
            message.into(),
            origin,
        ))
    }
}

impl IntoContextError<secp256k1::Error> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<secp256k1::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_secp256k1(source, message.into(), origin))
    }
}

impl IntoContextError<ToStrError> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<ToStrError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_to_str_error(source, message.into(), origin))
    }
}

impl IntoContextError<std::io::Error> for DiscoveryCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_io(source, message.into(), origin))
    }
}

impl IntoResponse for DiscoveryCrudError {
    fn into_response(self) -> Response {
        self.0.into_response()
    }
}
