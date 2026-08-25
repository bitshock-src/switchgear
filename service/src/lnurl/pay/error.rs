use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::http::header::InvalidHeaderValue;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::ContextErrorExt;
use switchgear_error::{ContextError, ErrorOrigin, IntoContextError};
use switchgear_service_api::balance::LnBalancerError;
use switchgear_service_api::lnurl::{LnUrlError, LnUrlErrorStatus};
use switchgear_service_api::offer::OfferStoreError;

const KIND_EVENT: &str = "event";
const CATEGORY_WEB: &str = "web";
const TYPE_ERROR: &str = "error";
const OUTCOME_FAILURE: &str = "failure";

pub enum LnUrlPayServiceErrorSourceKind {
    LnBalancer(Box<dyn LnBalancerError>),
    OfferStore(Box<dyn OfferStoreError>),
    InvalidHeaderValue(Box<InvalidHeaderValue>),
    UrlParse(Box<url::ParseError>),
    Io(Box<std::io::Error>),
    QrCode(Box<qrcode::types::QrError>),
    Image(Box<image::ImageError>),
    QueryRejection(Box<QueryRejection>),
    PathRejection(Box<PathRejection>),
    Uuid(Box<uuid::Error>),
}

impl Debug for LnUrlPayServiceErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LnBalancer(e) => f.debug_tuple("LnBalancer").field(&format!("{e}")).finish(),
            Self::OfferStore(e) => f.debug_tuple("OfferStore").field(&format!("{e}")).finish(),
            Self::InvalidHeaderValue(e) => f.debug_tuple("InvalidHeaderValue").field(e).finish(),
            Self::UrlParse(e) => f.debug_tuple("UrlParse").field(e).finish(),
            Self::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Self::QrCode(e) => f.debug_tuple("QrCode").field(e).finish(),
            Self::Image(e) => f.debug_tuple("Image").field(e).finish(),
            Self::QueryRejection(e) => f.debug_tuple("QueryRejection").field(e).finish(),
            Self::PathRejection(e) => f.debug_tuple("PathRejection").field(e).finish(),
            Self::Uuid(e) => f.debug_tuple("Uuid").field(e).finish(),
        }
    }
}

impl Display for LnUrlPayServiceErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LnBalancer(e) => Display::fmt(e, f),
            Self::OfferStore(e) => Display::fmt(e, f),
            Self::InvalidHeaderValue(e) => Display::fmt(e, f),
            Self::UrlParse(e) => Display::fmt(e, f),
            Self::Io(e) => Display::fmt(e, f),
            Self::QrCode(e) => Display::fmt(e, f),
            Self::Image(e) => Display::fmt(e, f),
            Self::QueryRejection(e) => Display::fmt(e, f),
            Self::PathRejection(e) => Display::fmt(e, f),
            Self::Uuid(e) => Display::fmt(e, f),
        }
    }
}

impl Error for LnUrlPayServiceErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LnBalancer(e) => Some(&**e),
            Self::OfferStore(e) => Some(&**e),
            Self::InvalidHeaderValue(e) => Some(&**e),
            Self::UrlParse(e) => Some(&**e),
            Self::Io(e) => Some(&**e),
            Self::QrCode(e) => Some(&**e),
            Self::Image(e) => Some(&**e),
            Self::QueryRejection(e) => Some(&**e),
            Self::PathRejection(e) => Some(&**e),
            Self::Uuid(e) => Some(&**e),
        }
    }
}

pub struct LnUrlPayServiceError {
    status: StatusCode,
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<LnUrlPayServiceErrorSourceKind>>,
}

impl Debug for LnUrlPayServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LnUrlPayServiceError")
            .field("status", &self.status)
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for LnUrlPayServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status, self.context)
    }
}

impl Error for LnUrlPayServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl ContextError for LnUrlPayServiceError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(LnUrlPayServiceErrorSourceKind::LnBalancer(e)) => Some(&**e),
            Some(LnUrlPayServiceErrorSourceKind::OfferStore(e)) => Some(&**e),
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

impl LnUrlPayServiceError {
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
                event.category = CATEGORY_WEB,
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
                event.category = CATEGORY_WEB,
                event.type = TYPE_ERROR,
                event.outcome = OUTCOME_FAILURE,
            );
        }
    }

    #[track_caller]
    fn with_source(
        source: LnUrlPayServiceErrorSourceKind,
        status: StatusCode,
        origin: ErrorOrigin,
        context: Cow<'static, str>,
    ) -> Self {
        let err = Self {
            status,
            context,
            origin,
            location: Location::caller(),
            source: Some(Box::new(source)),
        };
        err.emit_event();
        err
    }

    #[track_caller]
    fn sourceless(status: StatusCode, origin: ErrorOrigin, context: Cow<'static, str>) -> Self {
        let err = Self {
            status,
            context,
            origin,
            location: Location::caller(),
            source: None,
        };
        err.emit_event();
        err
    }

    #[track_caller]
    pub fn not_found<E>(error: E) -> Self
    where
        E: std::fmt::Display,
    {
        Self::sourceless(
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            error.to_string().into(),
        )
    }

    #[track_caller]
    pub fn bad_request<E>(error: E) -> Self
    where
        E: std::fmt::Display,
    {
        Self::sourceless(
            StatusCode::BAD_REQUEST,
            ErrorOrigin::Downstream,
            error.to_string().into(),
        )
    }

    #[track_caller]
    fn from_ln_balancer(
        source: Box<dyn LnBalancerError>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective_origin = origin.unwrap_or_else(|| source.origin());
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::LnBalancer(source),
            status_from_origin(effective_origin),
            effective_origin,
            context,
        )
    }

    #[track_caller]
    fn from_offer_store(
        source: Box<dyn OfferStoreError>,
        context: Cow<'static, str>,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective_origin = origin.unwrap_or_else(|| source.origin());
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::OfferStore(source),
            status_from_origin(effective_origin),
            effective_origin,
            context,
        )
    }
}

impl IntoContextError<InvalidHeaderValue> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<InvalidHeaderValue>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::InvalidHeaderValue(source),
            status_from_origin(origin),
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<url::ParseError> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<url::ParseError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::UrlParse(source),
            status_from_origin(origin),
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<std::io::Error> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::Io(source),
            status_from_origin(origin),
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<qrcode::types::QrError> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<qrcode::types::QrError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::QrCode(source),
            status_from_origin(origin),
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<image::ImageError> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<image::ImageError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or(ErrorOrigin::Internal);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::Image(source),
            status_from_origin(origin),
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<dyn LnBalancerError> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn LnBalancerError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::from_ln_balancer(source, message.into(), origin)
    }
}

impl IntoContextError<dyn OfferStoreError> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn OfferStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::from_offer_store(source, message.into(), origin)
    }
}

impl IntoResponse for LnUrlPayServiceError {
    fn into_response(self) -> Response {
        let reason = if self.status.is_server_error() {
            "internal server error".to_string()
        } else {
            self.context.into_owned()
        };
        let body = LnUrlError {
            status: LnUrlErrorStatus::Error,
            reason,
        };

        (self.status, axum::Json(body)).into_response()
    }
}

impl IntoContextError<QueryRejection> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<QueryRejection>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let status = source.status();
        let origin = origin.unwrap_or(ErrorOrigin::Downstream);
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::QueryRejection(source),
            status,
            origin,
            message.into(),
        )
    }
}

impl IntoContextError<PathRejection> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<PathRejection>,
        message: M,
        _origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::PathRejection(source),
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            message.into(),
        )
    }
}

impl IntoContextError<uuid::Error> for LnUrlPayServiceError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<uuid::Error>,
        message: M,
        _origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::with_source(
            LnUrlPayServiceErrorSourceKind::Uuid(source),
            StatusCode::NOT_FOUND,
            ErrorOrigin::Downstream,
            message.into(),
        )
    }
}
