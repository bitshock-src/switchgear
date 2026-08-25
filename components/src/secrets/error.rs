use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};

pub trait SecretContextError: ContextError {}

#[derive(Debug)]
pub enum SecretErrorSourceKind {
    Io(Box<std::io::Error>),
    Utf8(Box<std::str::Utf8Error>),
    UnknownSecret(String),
}

impl Display for SecretErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Utf8(e) => write!(f, "utf-8 decode error: {e}"),
            Self::UnknownSecret(name) => write!(f, "unknown secret: {name}"),
        }
    }
}

impl Error for SecretErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(&**e),
            Self::Utf8(e) => Some(&**e),
            Self::UnknownSecret(_) => None,
        }
    }
}

pub struct SecretError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<SecretErrorSourceKind>>,
}

impl Debug for SecretError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for SecretError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(f, "SecretError: while {}: {}", self.context, s),
            None => write!(f, "SecretError: {}", self.context),
        }
    }
}

impl Error for SecretError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl ContextError for SecretError {
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

impl SecretError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: SecretErrorSourceKind,
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
    pub(crate) fn unknown_secret<C: Into<Cow<'static, str>>>(name: String, context: C) -> Self {
        Self::new(
            SecretErrorSourceKind::UnknownSecret(name),
            ErrorOrigin::Downstream,
            context,
        )
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source_kind(&self) -> Option<&SecretErrorSourceKind> {
        self.source.as_deref()
    }
}

impl SecretContextError for SecretError {}

impl IntoBoxedTrait<dyn SecretContextError> for SecretError {
    fn into_boxed(self) -> Box<dyn SecretContextError> {
        Box::new(self)
    }
}

impl IntoContextError<std::io::Error> for SecretError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::io::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            SecretErrorSourceKind::Io(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}

impl IntoContextError<std::str::Utf8Error> for SecretError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<std::str::Utf8Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            SecretErrorSourceKind::Utf8(source),
            origin.unwrap_or(ErrorOrigin::Downstream),
            message,
        )
    }
}
