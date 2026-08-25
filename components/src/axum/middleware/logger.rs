use axum::extract::{ConnectInfo, Request};
use axum::http::Version;
use axum::response::Response;
use client_ip::{
    cf_connecting_ip, cloudfront_viewer_address, fly_client_ip, rightmost_forwarded,
    rightmost_x_forwarded_for, true_client_ip, x_real_ip,
};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};

pub const REQUEST_LOG_TARGET: &str = "swgr::request";

#[derive(Clone, Copy, Default)]
pub struct RequestLogger;

impl RequestLogger {
    pub const fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for RequestLogger {
    type Service = RequestLoggerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestLoggerService { inner }
    }
}

#[derive(Clone)]
pub struct RequestLoggerService<S> {
    inner: S,
}

impl<S> Service<Request> for RequestLoggerService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let not_ready_inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, not_ready_inner);

        Box::pin(async move {
            let method = req.method().clone();
            let uri = req.uri().clone();
            let version = match req.version() {
                Version::HTTP_09 => "0.9",
                Version::HTTP_10 => "1.0",
                Version::HTTP_11 => "1.1",
                Version::HTTP_2 => "2.0",
                Version::HTTP_3 => "3.0",
                _ => "1.1",
            };

            let client_ip = cf_connecting_ip(req.headers())
                .ok()
                .or_else(|| cloudfront_viewer_address(req.headers()).ok())
                .or_else(|| fly_client_ip(req.headers()).ok())
                .or_else(|| x_real_ip(req.headers()).ok())
                .or_else(|| true_client_ip(req.headers()).ok())
                .or_else(|| rightmost_forwarded(req.headers()).ok())
                .or_else(|| rightmost_x_forwarded_for(req.headers()).ok())
                .or_else(|| {
                    req.extensions()
                        .get::<ConnectInfo<SocketAddr>>()
                        .map(|ci| ci.ip())
                });

            let start = Instant::now();
            let response = inner.call(req).await?;

            let status_code = response.status().as_u16() as u64;
            let duration_ns = start.elapsed().as_nanos() as u64;

            let method_str = method.as_str();
            let path = uri.path();
            let query = uri.query();
            let client_ip_str = client_ip.map(|a| a.to_string());

            tracing::info!(
                target: REQUEST_LOG_TARGET,
                {
                    http.request.method = method_str,
                    http.response.status_code = status_code,
                    http.version = version,
                    url.path = path,
                    url.query = query,
                    client.ip = client_ip_str.as_deref(),
                    event.duration = duration_ns,
                },
                "request"
            );

            Ok(response)
        })
    }
}
