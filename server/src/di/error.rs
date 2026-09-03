use std::borrow::Cow;
use std::env::VarError;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::panic::Location;
use std::str::Utf8Error;

use opentelemetry_otlp::ExporterBuildError;
use opentelemetry_sdk::error::OTelSdkError;
use switchgear_components::secrets::SecretContextError;
use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};
use switchgear_service_api::discovery::DiscoveryBackendStoreError;
use switchgear_service_api::offer::OfferStoreError;
use tokio::task::JoinError;
use tracing_subscriber::filter::ParseError;
use tracing_subscriber::util::TryInitError;

pub trait DiContextError: ContextError {}

#[derive(Debug)]
pub enum DiErrorSourceKind {
    Io(Box<io::Error>),
    Utf8(Box<Utf8Error>),
    Saphyr(Box<serde_saphyr::Error>),
    Shellexpand(Box<shellexpand::LookupError<VarError>>),
    Dotenvy(Box<dotenvy::Error>),
    Pem(Box<rustls::pki_types::pem::Error>),
    Rustls(Box<rustls::Error>),
    Jwt(Box<jsonwebtoken::errors::Error>),
    TracingInit(Box<TryInitError>),
    FilterDirective(Box<ParseError>),
    OtlpExporterBuild(Box<ExporterBuildError>),
    OtelSdk(Box<OTelSdkError>),
    Join(Box<JoinError>),
    OfferStore(Box<dyn OfferStoreError>),
    DiscoveryBackendStore(Box<dyn DiscoveryBackendStoreError>),
    Di(Box<dyn DiContextError>),
    Secret(Box<dyn SecretContextError>),
}

impl Display for DiErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Utf8(e) => write!(f, "utf8 error: {e}"),
            Self::Saphyr(e) => write!(f, "yaml error: {e}"),
            Self::Shellexpand(e) => write!(f, "shell env expansion error: {e}"),
            Self::Dotenvy(e) => write!(f, "dotenv error: {e}"),
            Self::Pem(e) => write!(f, "pem error: {e}"),
            Self::Rustls(e) => write!(f, "rustls error: {e}"),
            Self::Jwt(e) => write!(f, "jwt error: {e}"),
            Self::TracingInit(e) => write!(f, "tracing init error: {e}"),
            Self::FilterDirective(e) => write!(f, "filter directive error: {e}"),
            Self::OtlpExporterBuild(e) => write!(f, "otlp exporter build error: {e}"),
            Self::OtelSdk(e) => write!(f, "otel sdk error: {e}"),
            Self::Join(e) => write!(f, "task join error: {e}"),
            Self::OfferStore(e) => Display::fmt(e, f),
            Self::DiscoveryBackendStore(e) => Display::fmt(e, f),
            Self::Di(e) => Display::fmt(e, f),
            Self::Secret(e) => Display::fmt(e, f),
        }
    }
}

impl Error for DiErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(&**e),
            Self::Utf8(e) => Some(&**e),
            Self::Saphyr(e) => Some(&**e),
            Self::Shellexpand(e) => Some(&**e),
            Self::Dotenvy(e) => Some(&**e),
            Self::Pem(e) => Some(&**e),
            Self::Rustls(e) => Some(&**e),
            Self::Jwt(e) => Some(&**e),
            Self::TracingInit(e) => Some(&**e),
            Self::FilterDirective(e) => Some(&**e),
            Self::OtlpExporterBuild(e) => Some(&**e),
            Self::OtelSdk(e) => Some(&**e),
            Self::Join(e) => Some(&**e),
            Self::OfferStore(e) => Some(&**e),
            Self::DiscoveryBackendStore(e) => Some(&**e),
            Self::Di(e) => Some(&**e),
            Self::Secret(e) => Some(&**e),
        }
    }
}

pub struct DiError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<DiErrorSourceKind>>,
}

impl Debug for DiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for DiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.context)
    }
}

impl Error for DiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().and_then(|s| s.source())
    }
}

impl ContextError for DiError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(DiErrorSourceKind::OfferStore(e)) => Some(&**e),
            Some(DiErrorSourceKind::DiscoveryBackendStore(e)) => Some(&**e),
            Some(DiErrorSourceKind::Di(e)) => Some(&**e),
            Some(DiErrorSourceKind::Secret(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl DiError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: DiErrorSourceKind,
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
    pub fn message<C: Into<Cow<'static, str>>>(origin: ErrorOrigin, context: C) -> Self {
        Self {
            context: context.into(),
            origin,
            location: Location::caller(),
            source: None,
        }
    }

    #[track_caller]
    pub fn internal<C: Into<Cow<'static, str>>>(context: C) -> Self {
        Self::message(ErrorOrigin::Internal, context)
    }
}

impl DiContextError for DiError {}

impl IntoBoxedTrait<dyn DiContextError> for DiError {
    fn into_boxed(self) -> Box<dyn DiContextError> {
        Box::new(self)
    }
}

impl IntoContextError<io::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Io(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<Utf8Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<Utf8Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Utf8(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<serde_saphyr::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_saphyr::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Saphyr(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<shellexpand::LookupError<VarError>> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<shellexpand::LookupError<VarError>>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Shellexpand(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<dotenvy::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dotenvy::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Dotenvy(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<rustls::pki_types::pem::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<rustls::pki_types::pem::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Pem(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<rustls::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<rustls::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Rustls(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<jsonwebtoken::errors::Error> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<jsonwebtoken::errors::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Jwt(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<TryInitError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<TryInitError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::TracingInit(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<ParseError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<ParseError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::FilterDirective(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<ExporterBuildError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<ExporterBuildError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::OtlpExporterBuild(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<OTelSdkError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<OTelSdkError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::OtelSdk(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<JoinError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<JoinError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            DiErrorSourceKind::Join(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<dyn OfferStoreError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn OfferStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(DiErrorSourceKind::OfferStore(source), effective, message)
    }
}

impl IntoContextError<dyn DiscoveryBackendStoreError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiscoveryBackendStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(
            DiErrorSourceKind::DiscoveryBackendStore(source),
            effective,
            message,
        )
    }
}

impl IntoContextError<dyn DiContextError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(DiErrorSourceKind::Di(source), effective, message)
    }
}

impl IntoContextError<dyn SecretContextError> for DiError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn SecretContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(DiErrorSourceKind::Secret(source), effective, message)
    }
}
