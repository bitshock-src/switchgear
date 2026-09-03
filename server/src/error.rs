use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;

use switchgear_error::{ContextError, ContextErrorExt, ErrorOrigin, IntoContextError};

use crate::commands::error::CliContextError;
use crate::di::error::DiContextError;

const KIND_EVENT: &str = "event";
const CATEGORY_PROCESS: &str = "process";
const TYPE_ERROR: &str = "error";
const OUTCOME_FAILURE: &str = "failure";

#[derive(Debug)]
pub enum ServerErrorSourceKind {
    Cli(Box<dyn CliContextError>),
    Di(Box<dyn DiContextError>),
}

impl Display for ServerErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli(e) => Display::fmt(e, f),
            Self::Di(e) => Display::fmt(e, f),
        }
    }
}

impl Error for ServerErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cli(e) => Some(&**e),
            Self::Di(e) => Some(&**e),
        }
    }
}

pub enum ServerErrorKind {
    Single,
    Multi(Vec<ServerError>),
}

pub struct ServerError {
    kind: ServerErrorKind,
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<ServerErrorSourceKind>>,
}

impl Debug for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .field(
                "children",
                &match &self.kind {
                    ServerErrorKind::Single => 0,
                    ServerErrorKind::Multi(children) => children.len(),
                },
            )
            .finish()
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.context)
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().and_then(|s| s.source())
    }
}

impl ContextError for ServerError {
    fn origin(&self) -> ErrorOrigin {
        self.origin
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(ServerErrorSourceKind::Cli(e)) => Some(&**e),
            Some(ServerErrorSourceKind::Di(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl ServerError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: ServerErrorSourceKind,
        origin: ErrorOrigin,
        context: C,
    ) -> Self {
        Self {
            kind: ServerErrorKind::Single,
            context: context.into(),
            origin,
            location: Location::caller(),
            source: Some(Box::new(source)),
        }
    }

    #[track_caller]
    pub fn message<C: Into<Cow<'static, str>>>(origin: ErrorOrigin, context: C) -> Self {
        Self {
            kind: ServerErrorKind::Single,
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
    pub fn multi(errors: Vec<ServerError>) -> Self {
        let flat: Vec<ServerError> = errors.into_iter().flat_map(ServerError::flatten).collect();
        Self {
            kind: ServerErrorKind::Multi(flat),
            context: Cow::Borrowed("multiple server errors"),
            origin: ErrorOrigin::Internal,
            location: Location::caller(),
            source: None,
        }
    }

    pub fn flatten(self) -> Vec<ServerError> {
        match self.kind {
            ServerErrorKind::Single => vec![self],
            ServerErrorKind::Multi(children) => {
                let mut out = Vec::with_capacity(children.len());
                for child in children {
                    out.extend(child.flatten());
                }
                out
            }
        }
    }

    pub fn emit_event(&self, prefix: &str) {
        let rendered = self.render_chain();
        let file = rendered.origin.file();
        let line = rendered.origin.line() as i64;
        let type_name = std::any::type_name::<Self>();
        let message = format!("{prefix}: {rendered}");
        let error_message = format!("{}: {}", prefix, self.context);

        tracing::error!(
            message,
            error.type = type_name,
            error.message = error_message,
            error.stack_trace = rendered.as_chain_string(),
            log.origin.file.name = file,
            log.origin.file.line = line,
            event.kind = KIND_EVENT,
            event.category = CATEGORY_PROCESS,
            event.type = TYPE_ERROR,
            event.outcome = OUTCOME_FAILURE,
        );
    }
}

impl IntoContextError<dyn CliContextError> for ServerError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn CliContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(ServerErrorSourceKind::Cli(source), effective, message)
    }
}

impl IntoContextError<dyn DiContextError> for ServerError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn DiContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(ServerErrorSourceKind::Di(source), effective, message)
    }
}

pub struct ServerErrorAccumulator {
    errors: Vec<ServerError>,
}

impl ServerErrorAccumulator {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn push(&mut self, err: ServerError) {
        self.errors.push(err);
    }

    pub fn push_result<T>(&mut self, r: Result<T, ServerError>) -> Option<T> {
        match r {
            Ok(t) => Some(t),
            Err(e) => {
                self.push(e);
                None
            }
        }
    }

    pub fn finish(mut self) -> Result<(), ServerError> {
        if self.errors.len() > 1 {
            return Err(ServerError::multi(self.errors));
        }
        match self.errors.pop() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

impl Default for ServerErrorAccumulator {
    fn default() -> Self {
        Self::new()
    }
}
