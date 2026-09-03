use std::borrow::Cow;
use std::env::VarError;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::panic::Location;
use std::time::SystemTimeError;

use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};
use switchgear_service_api::discovery::DiscoveryBackendStoreError;
use switchgear_service_api::offer::OfferStoreError;
use tokio::task::JoinError;

use crate::di::error::DiContextError;

pub trait CliContextError: ContextError {}

#[derive(Debug)]
pub enum CliErrorSourceKind {
    Io(Box<io::Error>),
    Env(Box<VarError>),
    Json(Box<serde_json::Error>),
    Jwt(Box<jsonwebtoken::errors::Error>),
    Pkcs8(Box<pkcs8::Error>),
    Spki(Box<pkcs8::spki::Error>),
    Pem(Box<rustls::pki_types::pem::Error>),
    Url(Box<url::ParseError>),
    SystemTime(Box<SystemTimeError>),
    Secp256k1(Box<secp256k1::Error>),
    Join(Box<JoinError>),
    OfferStore(Box<dyn OfferStoreError>),
    DiscoveryBackendStore(Box<dyn DiscoveryBackendStoreError>),
    Di(Box<dyn DiContextError>),
}

impl Display for CliErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Env(e) => write!(f, "env var error: {e}"),
            Self::Json(e) => write!(f, "json error: {e}"),
            Self::Jwt(e) => write!(f, "jwt error: {e}"),
            Self::Pkcs8(e) => write!(f, "pkcs8 error: {e}"),
            Self::Spki(e) => write!(f, "spki error: {e}"),
            Self::Pem(e) => write!(f, "pem error: {e}"),
            Self::Url(e) => write!(f, "url parse error: {e}"),
            Self::SystemTime(e) => write!(f, "system time error: {e}"),
            Self::Secp256k1(e) => write!(f, "secp256k1 error: {e}"),
            Self::Join(e) => write!(f, "task join error: {e}"),
            Self::OfferStore(e) => Display::fmt(e, f),
            Self::DiscoveryBackendStore(e) => Display::fmt(e, f),
            Self::Di(e) => Display::fmt(e, f),
        }
    }
}

impl Error for CliErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(&**e),
            Self::Env(e) => Some(&**e),
            Self::Json(e) => Some(&**e),
            Self::Jwt(e) => Some(&**e),
            Self::Pkcs8(e) => Some(&**e),
            Self::Spki(e) => Some(&**e),
            Self::Pem(e) => Some(&**e),
            Self::Url(e) => Some(&**e),
            Self::SystemTime(e) => Some(&**e),
            Self::Secp256k1(e) => Some(&**e),
            Self::Join(e) => Some(&**e),
            Self::OfferStore(e) => Some(&**e),
            Self::DiscoveryBackendStore(e) => Some(&**e),
            Self::Di(e) => Some(&**e),
        }
    }
}

pub enum CliErrorKind {
    Single,
    Multi(Vec<CliError>),
}

pub struct CliError {
    kind: CliErrorKind,
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<CliErrorSourceKind>>,
}

impl Debug for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .field(
                "children",
                &match &self.kind {
                    CliErrorKind::Single => 0,
                    CliErrorKind::Multi(children) => children.len(),
                },
            )
            .finish()
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.context)
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().and_then(|s| s.source())
    }
}

impl ContextError for CliError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(CliErrorSourceKind::OfferStore(e)) => Some(&**e),
            Some(CliErrorSourceKind::DiscoveryBackendStore(e)) => Some(&**e),
            Some(CliErrorSourceKind::Di(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl CliError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: CliErrorSourceKind,
        origin: ErrorOrigin,
        context: C,
    ) -> Self {
        Self {
            kind: CliErrorKind::Single,
            context: context.into(),
            origin,
            location: Location::caller(),
            source: Some(Box::new(source)),
        }
    }

    #[track_caller]
    pub fn message<C: Into<Cow<'static, str>>>(origin: ErrorOrigin, context: C) -> Self {
        Self {
            kind: CliErrorKind::Single,
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

    #[track_caller]
    pub fn multi(errors: Vec<CliError>) -> Self {
        let flat: Vec<CliError> = errors.into_iter().flat_map(CliError::flatten).collect();
        Self {
            kind: CliErrorKind::Multi(flat),
            context: Cow::Borrowed("multiple cli errors"),
            origin: ErrorOrigin::Internal,
            location: Location::caller(),
            source: None,
        }
    }

    pub fn flatten(self) -> Vec<CliError> {
        match self.kind {
            CliErrorKind::Single => vec![self],
            CliErrorKind::Multi(children) => {
                let mut out = Vec::with_capacity(children.len());
                for child in children {
                    out.extend(child.flatten());
                }
                out
            }
        }
    }
}

impl CliContextError for CliError {}

impl IntoBoxedTrait<dyn CliContextError> for CliError {
    fn into_boxed(self) -> Box<dyn CliContextError> {
        Box::new(self)
    }
}

impl IntoContextError<io::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Io(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<VarError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<VarError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Env(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<serde_json::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_json::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Json(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<jsonwebtoken::errors::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<jsonwebtoken::errors::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Jwt(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<pkcs8::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<pkcs8::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Pkcs8(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<pkcs8::spki::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<pkcs8::spki::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Spki(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<rustls::pki_types::pem::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<rustls::pki_types::pem::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Pem(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<url::ParseError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<url::ParseError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Url(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<SystemTimeError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<SystemTimeError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::SystemTime(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<secp256k1::Error> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<secp256k1::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Secp256k1(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<JoinError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<JoinError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            CliErrorSourceKind::Join(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<dyn OfferStoreError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn OfferStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(CliErrorSourceKind::OfferStore(source), effective, message)
    }
}

impl IntoContextError<dyn DiscoveryBackendStoreError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiscoveryBackendStoreError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(
            CliErrorSourceKind::DiscoveryBackendStore(source),
            effective,
            message,
        )
    }
}

impl IntoContextError<dyn DiContextError> for CliError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(CliErrorSourceKind::Di(source), effective, message)
    }
}

pub struct CliErrorAccumulator {
    errors: Vec<CliError>,
}

impl CliErrorAccumulator {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn push(&mut self, err: CliError) {
        self.errors.push(err);
    }

    pub fn push_result<T>(&mut self, r: Result<T, CliError>) -> Option<T> {
        match r {
            Ok(t) => Some(t),
            Err(e) => {
                self.push(e);
                None
            }
        }
    }

    pub fn finish(mut self) -> Result<(), CliError> {
        if self.errors.len() > 1 {
            return Err(CliError::multi(self.errors));
        }
        match self.errors.pop() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

impl Default for CliErrorAccumulator {
    fn default() -> Self {
        Self::new()
    }
}
