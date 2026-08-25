use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::HeaderValue;
use axum::http::header::InvalidHeaderValue;
use axum::response::{IntoResponse, Response};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoContextError};
use switchgear_service_api::offer::OfferStoreError;

use crate::axum::crud::error::CrudError;

pub struct OfferCrudError(CrudError);

impl OfferCrudError {
    #[track_caller]
    pub fn not_found() -> Self {
        Self(CrudError::not_found())
    }

    #[track_caller]
    pub fn bad() -> Self {
        Self(CrudError::bad())
    }

    #[track_caller]
    pub fn conflict(location: HeaderValue) -> Self {
        Self(CrudError::conflict(location))
    }
}

impl IntoContextError<JsonRejection> for OfferCrudError {
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

impl IntoContextError<QueryRejection> for OfferCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<QueryRejection>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_query_rejection(
            source,
            message.into(),
            origin,
        ))
    }
}

impl IntoContextError<PathRejection> for OfferCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<PathRejection>,
        message: M,
        _origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_path_rejection(source, message.into()))
    }
}

impl IntoContextError<uuid::Error> for OfferCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<uuid::Error>,
        message: M,
        _origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_uuid(source, message.into()))
    }
}

impl Debug for OfferCrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for OfferCrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Error for OfferCrudError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl ContextError for OfferCrudError {
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

impl IntoContextError<dyn OfferStoreError> for OfferCrudError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn OfferStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self(CrudError::from_offer_store(source, message.into(), origin))
    }
}

impl IntoContextError<InvalidHeaderValue> for OfferCrudError {
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

impl IntoResponse for OfferCrudError {
    fn into_response(self) -> Response {
        self.0.into_response()
    }
}
