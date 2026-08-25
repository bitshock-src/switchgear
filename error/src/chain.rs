use std::error::Error;
use std::fmt::{self, Write};
use std::panic::Location;

use crate::ContextError;

pub struct RenderedChain {
    pub chain: String,
    pub message_chain: String,
    pub origin: &'static Location<'static>,
}

impl RenderedChain {
    pub fn as_chain_string(&self) -> &str {
        &self.chain
    }
}

impl fmt::Display for RenderedChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message_chain)
    }
}

pub trait ContextErrorExt: ContextError {
    fn render_chain(&self) -> RenderedChain {
        render_chain(self)
    }
}

impl<T: ContextError + ?Sized> ContextErrorExt for T {}

pub fn render_chain<C: ContextError + ?Sized>(outer: &C) -> RenderedChain {
    let mut origin = outer.location();
    let outer_loc = origin;

    let mut chain = String::new();
    let mut message_chain = String::new();

    let _ = write!(message_chain, "{outer}");
    let _ = write!(chain, "{}:{}", outer_loc.file(), outer_loc.line());

    let mut ce_cursor: Option<&dyn ContextError> = outer.source_context();
    let mut foreign_start: Option<&(dyn Error + 'static)> = outer.source();

    while let Some(ce) = ce_cursor {
        let loc = ce.location();
        let _ = write!(message_chain, ": {ce}");
        let _ = write!(chain, ": {}:{}", loc.file(), loc.line());
        origin = loc;
        foreign_start = ce.source();
        ce_cursor = ce.source_context();
    }

    let mut walker = foreign_start;
    while let Some(node) = walker {
        let _ = write!(message_chain, ": {node}");
        let _ = write!(chain, ": {}", foreign_head(node));
        walker = node.source();
    }

    RenderedChain {
        chain,
        message_chain,
        origin,
    }
}

fn foreign_head(err: &(dyn Error + 'static)) -> String {
    let debug = format!("{err:?}");
    let word: String = debug
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if word.is_empty() {
        "[Error]".to_string()
    } else {
        format!("[Error: {word}]")
    }
}

#[cfg(test)]
mod tests {
    use super::render_chain;
    use crate::{ContextError, ErrorOrigin};
    use std::error::Error;
    use std::fmt;
    use std::io;
    use std::panic::Location;

    struct FakeContextError {
        message: &'static str,
        location: &'static Location<'static>,
        err_source: Option<Box<dyn Error + Send + Sync + 'static>>,
        ctx_source: Option<Box<dyn ContextError>>,
    }

    impl FakeContextError {
        #[track_caller]
        fn new(
            message: &'static str,
            err_source: Option<Box<dyn Error + Send + Sync + 'static>>,
            ctx_source: Option<Box<dyn ContextError>>,
        ) -> Self {
            Self {
                message,
                location: Location::caller(),
                err_source,
                ctx_source,
            }
        }
    }

    impl fmt::Debug for FakeContextError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("FakeContextError")
                .field("message", &self.message)
                .finish()
        }
    }

    impl fmt::Display for FakeContextError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.message)
        }
    }

    impl Error for FakeContextError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            if let Some(c) = self.ctx_source.as_deref() {
                Some(c as &(dyn Error + 'static))
            } else {
                self.err_source
                    .as_deref()
                    .map(|e| e as &(dyn Error + 'static))
            }
        }
    }

    impl ContextError for FakeContextError {
        fn origin(&self) -> ErrorOrigin {
            ErrorOrigin::Internal
        }
        fn location(&self) -> &'static Location<'static> {
            self.location
        }
        fn source_context(&self) -> Option<&dyn ContextError> {
            self.ctx_source.as_deref()
        }
    }

    fn frag(loc: &Location<'_>) -> String {
        format!("{}:{}", loc.file(), loc.line())
    }

    #[test]
    fn single_node_no_source() {
        let outer = FakeContextError::new("msg", None, None);
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "msg");
        assert_eq!(rendered.chain, frag(outer_loc));
        assert_eq!(rendered.as_chain_string(), rendered.chain);
        assert_eq!(rendered.to_string(), "msg");
        assert!(std::ptr::eq(rendered.origin, outer_loc));
    }

    #[test]
    fn outer_plus_foreign_leaf_only() {
        let leaf: Box<dyn Error + Send + Sync + 'static> = Box::new(io::Error::other("disk gone"));
        let outer = FakeContextError::new("msg", Some(leaf), None);
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "msg: disk gone");
        assert_eq!(
            rendered.chain,
            format!("{}: [Error: Custom]", frag(outer_loc))
        );
        assert_eq!(rendered.to_string(), "msg: disk gone");
        assert!(std::ptr::eq(rendered.origin, outer_loc));
    }

    #[test]
    fn outer_plus_context_source_no_further_source() {
        let inner: Box<dyn ContextError> = Box::new(FakeContextError::new("inner", None, None));
        let inner_loc = inner.location();
        let outer = FakeContextError::new("outer", None, Some(inner));
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "outer: inner");
        assert_eq!(
            rendered.chain,
            format!("{}: {}", frag(outer_loc), frag(inner_loc))
        );
        assert_eq!(rendered.to_string(), "outer: inner");
        assert!(std::ptr::eq(rendered.origin, inner_loc));
    }

    #[test]
    fn three_level_structured_chain() {
        let leaf: Box<dyn ContextError> = Box::new(FakeContextError::new("leaf", None, None));
        let leaf_loc = leaf.location();
        let mid: Box<dyn ContextError> = Box::new(FakeContextError::new("mid", None, Some(leaf)));
        let mid_loc = mid.location();
        let outer = FakeContextError::new("outer", None, Some(mid));
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "outer: mid: leaf");
        assert_eq!(
            rendered.chain,
            format!("{}: {}: {}", frag(outer_loc), frag(mid_loc), frag(leaf_loc))
        );
        assert_eq!(rendered.to_string(), "outer: mid: leaf");
        assert!(std::ptr::eq(rendered.origin, leaf_loc));
    }

    #[test]
    fn structured_then_foreign_transition_foolproof() {
        let leaf: Box<dyn Error + Send + Sync + 'static> = Box::new(io::Error::other("disk gone"));
        let mid: Box<dyn ContextError> = Box::new(FakeContextError::new("mid", Some(leaf), None));
        let mid_loc = mid.location();
        let outer = FakeContextError::new("outer", None, Some(mid));
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "outer: mid: disk gone");
        assert_eq!(
            rendered.chain,
            format!("{}: {}: [Error: Custom]", frag(outer_loc), frag(mid_loc))
        );
        assert_eq!(rendered.to_string(), "outer: mid: disk gone");
        assert!(std::ptr::eq(rendered.origin, mid_loc));
    }

    #[derive(Debug)]
    struct NestedErr {
        message: &'static str,
        source: Option<Box<NestedErr>>,
    }

    impl fmt::Display for NestedErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.message)
        }
    }

    impl Error for NestedErr {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref().map(|e| e as &(dyn Error + 'static))
        }
    }

    #[test]
    fn deep_foreign_chain_past_initial_foreign_transition() {
        let leaf = NestedErr {
            message: "leaf",
            source: None,
        };
        let mid = NestedErr {
            message: "mid",
            source: Some(Box::new(leaf)),
        };
        let top = NestedErr {
            message: "top",
            source: Some(Box::new(mid)),
        };
        let outer_source: Box<dyn Error + Send + Sync + 'static> = Box::new(top);
        let outer = FakeContextError::new("outer", Some(outer_source), None);
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.message_chain, "outer: top: mid: leaf");
        assert_eq!(
            rendered.chain,
            format!(
                "{}: [Error: NestedErr]: [Error: NestedErr]: [Error: NestedErr]",
                frag(outer_loc)
            )
        );
        assert_eq!(rendered.to_string(), "outer: top: mid: leaf");
        assert!(std::ptr::eq(rendered.origin, outer_loc));
    }

    #[test]
    fn foreign_head_empty_word_falls_back_to_bare_error_label() {
        struct WeirdDebug;
        impl fmt::Debug for WeirdDebug {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("  42 { code: 1 }")
            }
        }
        impl fmt::Display for WeirdDebug {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("weird")
            }
        }
        impl Error for WeirdDebug {}

        let leaf: Box<dyn Error + Send + Sync + 'static> = Box::new(WeirdDebug);
        let outer = FakeContextError::new("outer", Some(leaf), None);
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert_eq!(rendered.chain, format!("{}: [Error]", frag(outer_loc)));
        assert_eq!(rendered.message_chain, "outer: weird");
    }

    #[test]
    fn chain_terminator_no_dangling_separator() {
        let inner: Box<dyn ContextError> = Box::new(FakeContextError::new("inner", None, None));
        let inner_loc = inner.location();
        let outer = FakeContextError::new("outer", None, Some(inner));
        let outer_loc = outer.location();
        let rendered = render_chain(&outer);
        assert!(!rendered.message_chain.ends_with(": "));
        assert!(!rendered.chain.ends_with(": "));
        assert_eq!(rendered.message_chain, "outer: inner");
        assert_eq!(
            rendered.chain,
            format!("{}: {}", frag(outer_loc), frag(inner_loc))
        );
    }
}
