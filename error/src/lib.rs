pub mod chain;

pub use chain::{ContextErrorExt, RenderedChain, render_chain};

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::panic::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorOrigin {
    Downstream,
    Internal,
    Upstream,
}

impl fmt::Display for ErrorOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream => write!(f, "Upstream"),
            Self::Downstream => write!(f, "Downstream"),
            Self::Internal => write!(f, "Internal"),
        }
    }
}

pub trait ContextError: Error + Send + Sync + 'static {
    fn origin(&self) -> ErrorOrigin;
    fn location(&self) -> &'static Location<'static>;
    fn source_context(&self) -> Option<&dyn ContextError>;
}

pub trait IntoContextError<E>: ContextError
where
    E: Error + ?Sized,
{
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<E>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self;
}

pub trait IntoBoxedTrait<B: ?Sized> {
    fn into_boxed(self) -> Box<B>;
}

pub trait ForeignContext<T, E>: Sized
where
    E: Error + ?Sized,
{
    #[track_caller]
    fn foreign_context<U, M, O>(self, message: M, origin: O) -> Result<T, U>
    where
        U: IntoContextError<E>,
        M: Into<Cow<'static, str>>,
        O: Into<Option<ErrorOrigin>>;

    #[track_caller]
    fn with_foreign_context<U, M, F, O>(self, message: F, origin: O) -> Result<T, U>
    where
        U: IntoContextError<E>,
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M,
        O: Into<Option<ErrorOrigin>>;
}

impl<T, E> ForeignContext<T, E> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    #[track_caller]
    fn foreign_context<U, M, O>(self, message: M, origin: O) -> Result<T, U>
    where
        U: IntoContextError<E>,
        M: Into<Cow<'static, str>>,
        O: Into<Option<ErrorOrigin>>,
    {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(U::error(Box::new(e), message, origin.into())),
        }
    }

    #[track_caller]
    fn with_foreign_context<U, M, F, O>(self, message: F, origin: O) -> Result<T, U>
    where
        U: IntoContextError<E>,
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M,
        O: Into<Option<ErrorOrigin>>,
    {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(U::error(Box::new(e), message(), origin.into())),
        }
    }
}

pub trait ChainedContext<T, E, B: ?Sized>: Sized
where
    B: Error,
{
    #[track_caller]
    fn chained_context<U, M, O>(self, message: M, origin: O) -> Result<T, U>
    where
        U: IntoContextError<B>,
        M: Into<Cow<'static, str>>,
        O: Into<Option<ErrorOrigin>>;

    #[track_caller]
    fn with_chained_context<U, M, F, O>(self, message: F, origin: O) -> Result<T, U>
    where
        U: IntoContextError<B>,
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M,
        O: Into<Option<ErrorOrigin>>;
}

impl<T, E, B> ChainedContext<T, E, B> for Result<T, E>
where
    B: ContextError + ?Sized,
    E: IntoBoxedTrait<B>,
{
    #[track_caller]
    fn chained_context<U, M, O>(self, message: M, origin: O) -> Result<T, U>
    where
        U: IntoContextError<B>,
        M: Into<Cow<'static, str>>,
        O: Into<Option<ErrorOrigin>>,
    {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(U::error(e.into_boxed(), message, origin.into())),
        }
    }

    #[track_caller]
    fn with_chained_context<U, M, F, O>(self, message: F, origin: O) -> Result<T, U>
    where
        U: IntoContextError<B>,
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M,
        O: Into<Option<ErrorOrigin>>,
    {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(U::error(e.into_boxed(), message(), origin.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct ForeignTarget {
        message: Cow<'static, str>,
        source: Box<dyn Error + Send + Sync + 'static>,
        location: &'static Location<'static>,
    }

    impl fmt::Debug for ForeignTarget {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ForeignTarget")
                .field("message", &self.message)
                .finish()
        }
    }

    impl fmt::Display for ForeignTarget {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.message)
        }
    }

    impl Error for ForeignTarget {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&*self.source)
        }
    }

    impl ContextError for ForeignTarget {
        fn origin(&self) -> ErrorOrigin {
            ErrorOrigin::Internal
        }
        fn location(&self) -> &'static Location<'static> {
            self.location
        }
        fn source_context(&self) -> Option<&dyn ContextError> {
            None
        }
    }

    impl IntoContextError<io::Error> for ForeignTarget {
        #[track_caller]
        fn error<M: Into<Cow<'static, str>>>(
            source: Box<io::Error>,
            message: M,
            _origin: Option<ErrorOrigin>,
        ) -> Self {
            Self {
                message: message.into(),
                source: source as Box<dyn Error + Send + Sync + 'static>,
                location: Location::caller(),
            }
        }
    }

    #[test]
    fn foreign_context_preserves_original_source() {
        let r: Result<(), io::Error> = Err(io::Error::other("disk gone"));
        let err: ForeignTarget = r.foreign_context("reading", None).unwrap_err();
        let src = err.source().expect("source populated");
        assert_eq!(src.to_string(), "disk gone");
        assert!(src.downcast_ref::<io::Error>().is_some());
        assert_eq!(err.to_string(), "reading");
    }

    #[test]
    fn with_foreign_context_preserves_original_source() {
        let r: Result<(), io::Error> = Err(io::Error::other("disk gone"));
        let err: ForeignTarget = r
            .with_foreign_context(|| "reading lazily", None)
            .unwrap_err();
        let src = err.source().expect("source populated");
        assert_eq!(src.to_string(), "disk gone");
        assert_eq!(err.to_string(), "reading lazily");
    }

    #[test]
    fn foreign_context_captures_caller_location() {
        let r: Result<(), io::Error> = Err(io::Error::other("boom"));
        let expected_line = line!() + 1;
        let err: ForeignTarget = r.foreign_context("msg", None).unwrap_err();
        assert_eq!(err.location.file(), file!());
        assert_eq!(err.location.line(), expected_line);
    }

    #[test]
    fn with_foreign_context_captures_caller_location() {
        let r: Result<(), io::Error> = Err(io::Error::other("boom"));
        let expected_line = line!() + 1;
        let err: ForeignTarget = r.with_foreign_context(|| "msg", None).unwrap_err();
        assert_eq!(err.location.file(), file!());
        assert_eq!(err.location.line(), expected_line);
    }

    trait DummyContextTrait: ContextError {}

    struct ChildCtx {
        message: &'static str,
        location: &'static Location<'static>,
    }

    impl ChildCtx {
        #[track_caller]
        fn new(message: &'static str) -> Self {
            Self {
                message,
                location: Location::caller(),
            }
        }
    }

    impl fmt::Debug for ChildCtx {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ChildCtx")
                .field("message", &self.message)
                .finish()
        }
    }

    impl fmt::Display for ChildCtx {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.message)
        }
    }

    impl Error for ChildCtx {}

    impl ContextError for ChildCtx {
        fn origin(&self) -> ErrorOrigin {
            ErrorOrigin::Internal
        }
        fn location(&self) -> &'static Location<'static> {
            self.location
        }
        fn source_context(&self) -> Option<&dyn ContextError> {
            None
        }
    }

    impl DummyContextTrait for ChildCtx {}

    impl IntoBoxedTrait<dyn DummyContextTrait> for ChildCtx {
        fn into_boxed(self) -> Box<dyn DummyContextTrait> {
            Box::new(self)
        }
    }

    struct ChainedTarget {
        message: Cow<'static, str>,
        source: Box<dyn DummyContextTrait>,
        location: &'static Location<'static>,
    }

    impl fmt::Debug for ChainedTarget {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ChainedTarget")
                .field("message", &self.message)
                .finish()
        }
    }

    impl fmt::Display for ChainedTarget {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.message)
        }
    }

    impl Error for ChainedTarget {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&*self.source)
        }
    }

    impl ContextError for ChainedTarget {
        fn origin(&self) -> ErrorOrigin {
            ErrorOrigin::Internal
        }
        fn location(&self) -> &'static Location<'static> {
            self.location
        }
        fn source_context(&self) -> Option<&dyn ContextError> {
            Some(&*self.source)
        }
    }

    impl IntoContextError<dyn DummyContextTrait> for ChainedTarget {
        #[track_caller]
        fn error<M: Into<Cow<'static, str>>>(
            source: Box<dyn DummyContextTrait>,
            message: M,
            _origin: Option<ErrorOrigin>,
        ) -> Self {
            Self {
                message: message.into(),
                source,
                location: Location::caller(),
            }
        }
    }

    #[test]
    fn chained_context_preserves_child_in_source_context() {
        let child = ChildCtx::new("child");
        let r: Result<(), ChildCtx> = Err(child);
        let err: ChainedTarget = r.chained_context("outer", None).unwrap_err();
        let src = err.source_context().expect("source_context populated");
        assert_eq!(src.to_string(), "child");
        assert_eq!(err.to_string(), "outer");
    }

    #[test]
    fn with_chained_context_preserves_child_in_source_context() {
        let child = ChildCtx::new("child");
        let r: Result<(), ChildCtx> = Err(child);
        let err: ChainedTarget = r.with_chained_context(|| "outer lazily", None).unwrap_err();
        let src = err.source_context().expect("source_context populated");
        assert_eq!(src.to_string(), "child");
        assert_eq!(err.to_string(), "outer lazily");
    }

    #[test]
    fn chained_context_captures_caller_location() {
        let child = ChildCtx::new("child");
        let r: Result<(), ChildCtx> = Err(child);
        let expected_line = line!() + 1;
        let err: ChainedTarget = r.chained_context("outer", None).unwrap_err();
        assert_eq!(err.location.file(), file!());
        assert_eq!(err.location.line(), expected_line);
    }

    #[test]
    fn with_chained_context_captures_caller_location() {
        let child = ChildCtx::new("child");
        let r: Result<(), ChildCtx> = Err(child);
        let expected_line = line!() + 1;
        let err: ChainedTarget = r.with_chained_context(|| "outer", None).unwrap_err();
        assert_eq!(err.location.file(), file!());
        assert_eq!(err.location.line(), expected_line);
    }
}
