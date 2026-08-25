use axum::extract::Request;
use axum::response::Response;
use std::task::{Context, Poll};
use tower::{Layer, Service};

const TRACE_CONTEXT_HEADERS: &[&str] = &["traceparent", "tracestate", "baggage"];

#[derive(Clone, Copy, Default)]
pub struct StripTraceContextLayer;

impl StripTraceContextLayer {
    pub const fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for StripTraceContextLayer {
    type Service = StripTraceContextService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        StripTraceContextService { inner }
    }
}

#[derive(Clone)]
pub struct StripTraceContextService<S> {
    inner: S,
}

impl<S> Service<Request> for StripTraceContextService<S>
where
    S: Service<Request, Response = Response>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let headers = req.headers_mut();
        for name in TRACE_CONTEXT_HEADERS {
            headers.remove(*name);
        }
        self.inner.call(req)
    }
}
