use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::IntoBoxedTrait;
use switchgear_error::{ContextError, ErrorOrigin, IntoContextError};
use switchgear_service_api::balance::LnBalancerError;

pub enum PingoraLnErrorSourceKind {
    ServiceError(Box<dyn crate::PingoraLnClientPoolError>),
}

impl Debug for PingoraLnErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServiceError(e) => f
                .debug_tuple("ServiceError")
                .field(&format!("{e}"))
                .finish(),
        }
    }
}

impl Display for PingoraLnErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServiceError(e) => Display::fmt(e, f),
        }
    }
}

impl Error for PingoraLnErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ServiceError(e) => Some(&**e),
        }
    }
}

pub struct PingoraLnError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<PingoraLnErrorSourceKind>>,
}

impl Debug for PingoraLnError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PingoraLnError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for PingoraLnError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(f, "PingoraLnError: while {}: {}", self.context, s),
            None => write!(f, "PingoraLnError: {}", self.context),
        }
    }
}

impl Error for PingoraLnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl PingoraLnError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: PingoraLnErrorSourceKind,
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

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source_kind(&self) -> Option<&PingoraLnErrorSourceKind> {
        self.source.as_deref()
    }
}

impl ContextError for PingoraLnError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(PingoraLnErrorSourceKind::ServiceError(e)) => Some(&**e),
            None => None,
        }
    }
}

impl LnBalancerError for PingoraLnError {}

impl IntoBoxedTrait<dyn LnBalancerError> for PingoraLnError {
    fn into_boxed(self) -> Box<dyn LnBalancerError> {
        Box::new(self)
    }
}

impl crate::PingoraLnClientPoolError for PingoraLnError {}

impl IntoBoxedTrait<dyn crate::PingoraLnClientPoolError> for PingoraLnError {
    fn into_boxed(self) -> Box<dyn crate::PingoraLnClientPoolError> {
        Box::new(self)
    }
}

impl IntoContextError<dyn crate::PingoraLnClientPoolError> for PingoraLnError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn crate::PingoraLnClientPoolError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(
            PingoraLnErrorSourceKind::ServiceError(source),
            effective,
            message,
        )
    }
}
