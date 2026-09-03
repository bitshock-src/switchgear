use crate::pool::LnRpcClientError;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};
use tonic::{Code, Status, transport};

#[derive(Debug)]
pub(crate) enum LnPoolErrorSourceKind {
    TonicError(Box<Status>),
    TransportError(Box<transport::Error>),
    JsonError(Box<serde_json::Error>),
    Io(Box<std::io::Error>),
    InvalidUri(Box<http::uri::InvalidUri>),
    SystemTime(Box<std::time::SystemTimeError>),
    Pem(Box<rustls::pki_types::pem::Error>),
    Rustls(Box<rustls::Error>),
}

impl Display for LnPoolErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TonicError(e) => Display::fmt(e, f),
            Self::TransportError(e) => Display::fmt(e, f),
            Self::JsonError(e) => Display::fmt(e, f),
            Self::Io(e) => Display::fmt(e, f),
            Self::InvalidUri(e) => Display::fmt(e, f),
            Self::SystemTime(e) => Display::fmt(e, f),
            Self::Pem(e) => Display::fmt(e, f),
            Self::Rustls(e) => Display::fmt(e, f),
        }
    }
}

impl Error for LnPoolErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TonicError(e) => Some(&**e),
            Self::TransportError(e) => Some(&**e),
            Self::JsonError(e) => Some(&**e),
            Self::Io(e) => Some(&**e),
            Self::InvalidUri(e) => Some(&**e),
            Self::SystemTime(e) => Some(&**e),
            Self::Pem(e) => Some(&**e),
            Self::Rustls(e) => Some(&**e),
        }
    }
}

pub struct LnPoolError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<LnPoolErrorSourceKind>>,
}

impl Debug for LnPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LnPoolError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for LnPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(f, "LnPoolError: while {}: {}", self.context, s),
            None => write!(f, "LnPoolError: {}", self.context),
        }
    }
}

impl Error for LnPoolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl LnPoolError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: LnPoolErrorSourceKind,
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
}

impl ContextError for LnPoolError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        None
    }
}

impl LnRpcClientError for LnPoolError {}

impl IntoBoxedTrait<dyn LnRpcClientError> for LnPoolError {
    fn into_boxed(self) -> Box<dyn LnRpcClientError> {
        Box::new(self)
    }
}

fn origin_from_tonic_status(status: &Status) -> ErrorOrigin {
    match status.code() {
        Code::InvalidArgument | Code::OutOfRange | Code::AlreadyExists => ErrorOrigin::Downstream,
        _ => ErrorOrigin::Upstream,
    }
}

impl IntoContextError<Status> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<Status>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let origin = origin.unwrap_or_else(|| origin_from_tonic_status(&source));
        Self::new(LnPoolErrorSourceKind::TonicError(source), origin, message)
    }
}

impl IntoContextError<transport::Error> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<transport::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::TransportError(source),
            origin.unwrap_or(ErrorOrigin::Upstream),
            message,
        )
    }
}

impl IntoContextError<serde_json::Error> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_json::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::JsonError(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<std::io::Error> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::Io(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<http::uri::InvalidUri> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<http::uri::InvalidUri>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::InvalidUri(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<std::time::SystemTimeError> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::time::SystemTimeError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::SystemTime(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<rustls::pki_types::pem::Error> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<rustls::pki_types::pem::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::Pem(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<rustls::Error> for LnPoolError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<rustls::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::Rustls(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}
