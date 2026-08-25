use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::header::{InvalidHeaderValue, ToStrError};
use axum::http::{HeaderMap, HeaderValue};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ContextErrorExt, ErrorOrigin};
use switchgear_service_api::discovery::DiscoveryBackendStoreError;
use switchgear_service_api::offer::OfferStoreError;

const KIND_EVENT: &str = "event";
const CATEGORY_API_WEB: &str = r#"["api","web"]"#;
const TYPE_ERROR: &str = "error";
const OUTCOME_FAILURE: &str = "failure";

#[derive(Debug)]
pub enum WwwAuthenticateError {
    MissingToken,
    InvalidToken,
}

impl Display for WwwAuthenticateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WwwAuthenticateError::MissingToken => write!(f, "missing_token"),
            WwwAuthenticateError::InvalidToken => write!(f, "invalid_token"),
        }
    }
}

pub enum CrudErrorSourceKind {
    OfferStore(Box<dyn OfferStoreError>),
    DiscoveryBackendStore(Box<dyn DiscoveryBackendStoreError>),
    JsonRejection(Box<JsonRejection>),
    QueryRejection(Box<QueryRejection>),
    PathRejection(Box<PathRejection>),
    Uuid(Box<uuid::Error>),
    InvalidHeaderValue(Box<InvalidHeaderValue>),
    Io(Box<std::io::Error>),
    Secp256k1(Box<secp256k1::Error>),
    ToStrError(Box<ToStrError>),
}

impl Debug for CrudErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OfferStore(e) => f.debug_tuple("OfferStore").field(&format!("{e}")).finish(),
            Self::DiscoveryBackendStore(e) => f
                .debug_tuple("DiscoveryBackendStore")
                .field(&format!("{e}"))
                .finish(),
            Self::JsonRejection(e) => f.debug_tuple("JsonRejection").field(e).finish(),
            Self::QueryRejection(e) => f.debug_tuple("QueryRejection").field(e).finish(),
            Self::PathRejection(e) => f.debug_tuple("PathRejection").field(e).finish(),
            Self::Uuid(e) => f.debug_tuple("Uuid").field(e).finish(),
            Self::InvalidHeaderValue(e) => f.debug_tuple("InvalidHeaderValue").field(e).finish(),
            Self::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Self::Secp256k1(e) => f.debug_tuple("Secp256k1").field(e).finish(),
            Self::ToStrError(e) => f.debug_tuple("ToStrError").field(e).finish(),
        }
    }
}

impl Display for CrudErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OfferStore(e) => Display::fmt(e, f),
            Self::DiscoveryBackendStore(e) => Display::fmt(e, f),
            Self::JsonRejection(e) => Display::fmt(e, f),
            Self::QueryRejection(e) => Display::fmt(e, f),
            Self::PathRejection(e) => Display::fmt(e, f),
            Self::Uuid(e) => Display::fmt(e, f),
            Self::InvalidHeaderValue(e) => Display::fmt(e, f),
            Self::Io(e) => Display::fmt(e, f),
            Self::Secp256k1(e) => Display::fmt(e, f),
            Self::ToStrError(e) => Display::fmt(e, f),
        }
    }
}

impl Error for CrudErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OfferStore(e) => Some(&**e),
            Self::DiscoveryBackendStore(e) => Some(&**e),
            Self::JsonRejection(e) => Some(&**e),
            Self::QueryRejection(e) => Some(&**e),
            Self::PathRejection(e) => Some(&**e),
            Self::Uuid(e) => Some(&**e),
            Self::InvalidHeaderValue(e) => Some(&**e),
            Self::Io(e) => Some(&**e),
            Self::Secp256k1(e) => Some(&**e),
            Self::ToStrError(e) => Some(&**e),
        }
    }
}

pub struct CrudError {
    status: StatusCode,
    headers: Option<Box<HeaderMap>>,
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<CrudErrorSourceKind>>,
}

impl Debug for CrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrudError")
            .field("status", &self.status)
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for CrudError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.status)
    }
}

impl Error for CrudError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl ContextError for CrudError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(CrudErrorSourceKind::OfferStore(e)) => Some(&**e),
            Some(CrudErrorSourceKind::DiscoveryBackendStore(e)) => Some(&**e),
            _ => None,
        }
    }
}

fn status_from_origin(origin: ErrorOrigin) -> StatusCode {
    match origin {
        ErrorOrigin::Downstream => StatusCode::BAD_REQUEST,
        ErrorOrigin::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorOrigin::Upstream => StatusCode::BAD_GATEWAY,
    }
}

impl CrudError {
    fn emit_event(&self) {
        let rendered = self.render_chain();
        let status_code = self.status.as_u16() as i64;
        let file = rendered.origin.file();
        let line = rendered.origin.line() as i64;
        let type_name = std::any::type_name::<Self>();

        if self.status.is_server_error() {
            tracing::error!(
                http.response.status_code = status_code,
                message = %rendered,
                error.type = type_name,
                error.message = %self.context,
                error.stack_trace = rendered.as_chain_string(),
                log.origin.file.name = file,
                log.origin.file.line = line,
                event.kind = KIND_EVENT,
                event.category = CATEGORY_API_WEB,
                event.type = TYPE_ERROR,
                event.outcome = OUTCOME_FAILURE,
            );
        } else {
            tracing::warn!(
                http.response.status_code = status_code,
                message = %rendered,
                error.type = type_name,
                error.message = %self.context,
                error.stack_trace = rendered.as_chain_string(),
                log.origin.file.name = file,
                log.origin.file.line = line,
                event.kind = KIND_EVENT,
                event.category = CATEGORY_API_WEB,
                event.type = TYPE_ERROR,
                event.outcome = OUTCOME_FAILURE,
            );
        }
    }

    #[track_caller]
    fn with_source(
        source: CrudErrorSourceKind,
        status: StatusCode,
        origin: ErrorOrigin,
        context: Cow<'static, str>,
        headers: Option<Box<HeaderMap>>,
    ) -> Self {
        let err = Self {
            status,
            headers,
            context,
            origin,
            location: Location::caller(),
            source: Some(Box::new(source)),
        };
        err.emit_event();
        err
    }

    #[track_caller]
    fn sourceless(
        status: StatusCode,
        origin: ErrorOrigin,
        context: Cow<'static, str>,
        headers: Option<Box<HeaderMap>>,
    ) -> Self {
        let err = Self {
            status,
            headers,
            context,
            origin,
            location: Location::caller(),
            source: None,
        };
        err.emit_event();
        err
    }

    #[track_caller]
    pub fn not_found() -> Self {
        Self::sourceless(
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            Cow::Borrowed("not found"),
            None,
        )
    }

    #[track_caller]
    pub fn bad() -> Self {
        Self::sourceless(
            StatusCode::BAD_REQUEST,
            ErrorOrigin::Downstream,
            Cow::Borrowed("bad request"),
            None,
        )
    }

    #[track_caller]
    pub fn conflict(location: HeaderValue) -> Self {
        Self::sourceless(
            StatusCode::CONFLICT,
            ErrorOrigin::Downstream,
            Cow::Borrowed("conflict"),
            Some(Box::new(HeaderMap::from_iter(vec![(
                axum::http::header::LOCATION,
                location,
            )]))),
        )
    }

    #[track_caller]
    pub fn unauthorized(realm: &str, error: WwwAuthenticateError) -> Self {
        let value = match error {
            WwwAuthenticateError::MissingToken => {
                HeaderValue::from_str(&format!(r#"Bearer realm="{realm}""#))
            }
            WwwAuthenticateError::InvalidToken => {
                HeaderValue::from_str(&format!(r#"Bearer realm="{realm}", error="{error}""#))
            }
        };

        let value = value.unwrap_or_else(|_| HeaderValue::from_static("Bearer"));

        Self::sourceless(
            StatusCode::UNAUTHORIZED,
            ErrorOrigin::Downstream,
            Cow::Borrowed("unauthorized"),
            Some(Box::new(HeaderMap::from_iter(vec![(
                axum::http::header::WWW_AUTHENTICATE,
                value,
            )]))),
        )
    }

    #[track_caller]
    pub fn from_offer_store(
        source: Box<dyn OfferStoreError>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective_origin = origin.unwrap_or_else(|| source.origin());
        Self::with_source(
            CrudErrorSourceKind::OfferStore(source),
            status_from_origin(effective_origin),
            effective_origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_discovery_backend_store(
        source: Box<dyn DiscoveryBackendStoreError>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective_origin = origin.unwrap_or_else(|| source.origin());
        Self::with_source(
            CrudErrorSourceKind::DiscoveryBackendStore(source),
            status_from_origin(effective_origin),
            effective_origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_json_rejection(
        source: Box<JsonRejection>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let status = source.status();
        let origin = origin.unwrap_or(ErrorOrigin::Downstream);
        Self::with_source(
            CrudErrorSourceKind::JsonRejection(source),
            status,
            origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_query_rejection(
        source: Box<QueryRejection>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let status = source.status();
        let origin = origin.unwrap_or(ErrorOrigin::Downstream);
        Self::with_source(
            CrudErrorSourceKind::QueryRejection(source),
            status,
            origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_path_rejection(source: Box<PathRejection>, context: Cow<'static, str>) -> Self {
        Self::with_source(
            CrudErrorSourceKind::PathRejection(source),
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_uuid(source: Box<uuid::Error>, context: Cow<'static, str>) -> Self {
        Self::with_source(
            CrudErrorSourceKind::Uuid(source),
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_invalid_header_value(
        source: Box<InvalidHeaderValue>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            CrudErrorSourceKind::InvalidHeaderValue(source),
            status_from_origin(origin),
            origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_io(
        source: Box<std::io::Error>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            CrudErrorSourceKind::Io(source),
            status_from_origin(origin),
            origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_secp256k1(
        source: Box<secp256k1::Error>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Downstream);
        Self::with_source(
            CrudErrorSourceKind::Secp256k1(source),
            status_from_origin(origin),
            origin,
            context,
            None,
        )
    }

    #[track_caller]
    pub fn from_to_str_error(
        source: Box<ToStrError>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Downstream);
        Self::with_source(
            CrudErrorSourceKind::ToStrError(source),
            status_from_origin(origin),
            origin,
            context,
            None,
        )
    }

    pub fn into_response(self) -> Response {
        let headers = self.headers.map(|h| *h).unwrap_or_default();
        (headers, self.status).into_response()
    }
}

pub fn unauthorized_response(realm: &str, error: WwwAuthenticateError) -> Response {
    CrudError::unauthorized(realm, error).into_response()
}
