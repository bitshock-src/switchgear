# ContextError Template

Canonical structure for every production `Error + ContextError` implementation in
this repo. New error types MUST match this template.

The runtime traits (`ContextError`, `IntoContextError`, `ForeignContext`,
`ChainedContext`, `IntoBoxedTrait`) live in this crate's `src/lib.rs`; the
chain-rendering helpers used in tests live in `src/chain.rs`. The template
below uses `XxxContextError` as a placeholder for any domain-specific supertrait of
`ContextError` (`pub trait XxxContextError: ContextError {}`), and `XxxError` /
`XxxErrorSourceKind` as placeholders for the concrete outer / inner types.

---

## Rules

1. **Foreign errors are boxed concrete types in the inner enum.**
   Every non-`ContextError` source (e.g. `sqlx::Error`, `std::io::Error`,
   `reqwest::Error`, `tonic::Status`) is stored as `Box<ConcreteType>` inside a
   variant of the inner source-kind enum — never inline (`Concrete`) and never
   erased to `Box<dyn Error>`.

2. **`ContextError` sources are boxed and stored as the marker trait, not
   `dyn ContextError`.**
   Where a source is itself a `ContextError` produced by another layer, the
   inner enum variant stores `Box<dyn XxxContextError>` — a domain-specific
   supertrait of `ContextError` (`pub trait XxxContextError: ContextError {}`)
   defined by the layer that owns the error. Add one enum variant per marker
   trait you accept. Never `Box<dyn ContextError>` — that erases the marker
   and defeats `IntoBoxedTrait` / trait-bounded APIs.

3. **Inner enum container, optional on the outer struct.**
   Every outer error owns at most one boxed inner enum
   (`source: Option<Box<XxxErrorSourceKind>>`) that discriminates every
   possible source. The enum implements `std::fmt::Display + std::error::Error`
   and forwards `Error::source()` to the boxed concrete for foreign variants
   and to the boxed marker trait for context variants. It does NOT implement
   `ContextError`. Sourceless errors set `source: None` and put their prose in
   `context`; see rule 5.

4. **Outer container holds the ContextError metadata (and only that).**
   The outer struct's fields are exactly what's needed to satisfy
   `ContextError` + convenience:
   - `context: Cow<'static, str>` — the human message
   - `origin: ErrorOrigin` — the origin enum
   - `location: &'static Location<'static>` — captured via `#[track_caller]`
   - `source: Option<Box<XxxErrorSourceKind>>` — the inner enum, `None` for
     sourceless errors
   Any additional cross-cutting fields required by an unrelated concern
   (e.g. HTTP `status`/`headers` for axum response types) live on the outer
   container as well, but never inside the source-kind enum.

5. **No sentinel / message-only variants unless a consumer matches on them.**
   Variants that carry no source (e.g. `Poisoned`, `NotFound`,
   `HttpStatus(u16)`, `Internal(String)`, `NoAvailableNodes`, `Error(String)`,
   `Message(String)`) are ONLY justified if some caller actually pattern-matches
   on the discriminant to steer call flow (retry, HTTP mapping, test
   assertion, etc.). If no consumer does, delete the variant and represent the
   error via `source: None` + a descriptive `context` string on the outer
   struct. This keeps the source-kind enum a tight, non-lying discriminator
   over real source types.

---

## Canonical form

```rust
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};

// -------- Inner source-kind enum --------

#[derive(Debug)]
pub enum XxxErrorSourceKind {
    // Foreign errors: boxed concrete types.
    Database(Box<sqlx::Error>),
    Http(Box<reqwest::Error>),
    Io(Box<std::io::Error>),

    // ContextError sources: boxed marker trait (NOT dyn ContextError).
    // Add one variant per marker trait you accept.
    Xxx(Box<dyn XxxContextError>),

    // Sourceless discriminants ONLY when a real consumer matches on them.
    // Delete anything without a match consumer — see rule 5.
    InvalidInput(String),
}

impl Display for XxxErrorSourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Http(e) => write!(f, "http error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Xxx(e) => Display::fmt(e, f),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl Error for XxxErrorSourceKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(e) => Some(&**e),
            Self::Http(e)     => Some(&**e),
            Self::Io(e)       => Some(&**e),
            Self::Xxx(e)   => Some(&**e), // dyn XxxContextError: ContextError: Error
            Self::InvalidInput(_) => None,
        }
    }
}

// -------- Outer wrapper --------

pub struct XxxError {
    context: Cow<'static, str>,
    origin: ErrorOrigin,
    location: &'static Location<'static>,
    source: Option<Box<XxxErrorSourceKind>>,
}

impl Debug for XxxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XxxError")
            .field("context", &self.context)
            .field("origin", &self.origin)
            .field("source", &self.source)
            .finish()
    }
}

impl Display for XxxError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(s) => write!(f, "XxxError: while {}: {}", self.context, s),
            None    => write!(f, "XxxError: {}", self.context),
        }
    }
}

impl Error for XxxError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn Error + 'static))
    }
}

impl ContextError for XxxError {
    fn origin(&self) -> ErrorOrigin { self.origin }
    fn location(&self) -> &'static Location<'static> { self.location }
    fn source_context(&self) -> Option<&dyn ContextError> {
        match self.source.as_deref() {
            Some(XxxErrorSourceKind::Xxx(e)) => Some(&**e),
            _ => None,
        }
    }
}

impl XxxError {
    #[track_caller]
    fn new<C: Into<Cow<'static, str>>>(
        source: XxxErrorSourceKind,
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

    // Sourceless constructor for prose-only errors (e.g. "poisoned lock").
    #[track_caller]
    fn message<C: Into<Cow<'static, str>>>(origin: ErrorOrigin, context: C) -> Self {
        Self {
            context: context.into(),
            origin,
            location: Location::caller(),
            source: None,
        }
    }
}

// -------- Marker trait plumbing (if the type is exposed via one) --------

impl XxxContextError for XxxError {}
impl IntoBoxedTrait<dyn XxxContextError> for XxxError {
    fn into_boxed(self) -> Box<dyn XxxContextError> { Box::new(self) }
}

// -------- IntoContextError impls: one per foreign type & marker trait --------

impl IntoContextError<sqlx::Error> for XxxError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<sqlx::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self::new(
            XxxErrorSourceKind::Database(source),
            origin.unwrap_or(ErrorOrigin::Internal),
            message,
        )
    }
}

impl IntoContextError<dyn XxxContextError> for XxxError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<dyn XxxContextError>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        let effective = origin.unwrap_or_else(|| source.origin());
        Self::new(XxxErrorSourceKind::Xxx(source), effective, message)
    }
}
```

---

## Forbidden patterns

- ❌ Storing foreign errors inline: `Database(sqlx::Error)` → ✅ `Database(Box<sqlx::Error>)`.
- ❌ Storing context sources as `Box<dyn ContextError>` → ✅ variant per marker trait holding `Box<dyn MarkerTrait>`.
- ❌ Inline (`source: XxxErrorSourceKind`) inner enum on the outer struct → ✅ boxed and optional (`source: Option<Box<XxxErrorSourceKind>>`).
- ❌ String-only "generic" variants when a real typed source exists (e.g. `Error(String)` wrapping a formatted foreign error) → ✅ typed variant carrying the boxed concrete.
- ❌ Sentinel / message-only variants that no consumer pattern-matches on (e.g. `Poisoned`, `NotFound`, `HttpStatus(u16)`, `Internal(String)`, `Message(String)`) → ✅ delete the variant; use `source: None` with prose in `context`.

## Naming conventions

- Outer struct: `<Domain>Error`.
- Inner enum: always `<Domain>ErrorSourceKind` — never `<Domain>ErrorSource`
  and never a bare `SourceKind`.
- Origin field on the outer struct: always `origin: ErrorOrigin`.
- Public origin accessor: none — callers use `ContextError::origin`. Do not
  add duplicate helpers.
