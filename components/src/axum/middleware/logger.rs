use axum::extract::{ConnectInfo, Request};
use axum::http::Version;
use axum::response::Response;
use chrono::{DateTime, Utc};
use client_ip::{
    cf_connecting_ip, cloudfront_viewer_address, fly_client_ip, rightmost_forwarded,
    rightmost_x_forwarded_for, true_client_ip, x_real_ip,
};
use log::{log, Level};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct ClfLogger {
    service_log_target: String,
}

impl ClfLogger {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_log_target: format!("clf::{service_name}"),
        }
    }
}

impl<S> Layer<S> for ClfLogger {
    type Service = ClfLoggerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ClfLoggerService {
            inner,
            service_log_target: self.service_log_target.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ClfLoggerService<S> {
    inner: S,
    service_log_target: String,
}

impl<S> Service<Request> for ClfLoggerService<S>
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

        let service_name = self.service_log_target.clone();

        Box::pin(async move {
            let method = req.method().clone();
            let uri = req.uri().clone();
            let version = match req.version() {
                Version::HTTP_09 => "HTTP/0.9",
                Version::HTTP_10 => "HTTP/1.0",
                Version::HTTP_11 => "HTTP/1.1",
                Version::HTTP_2 => "HTTP/2.0",
                Version::HTTP_3 => "HTTP/3.0",
                _ => "HTTP/1.1",
            };

            let host = cf_connecting_ip(req.headers())
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

            let host = host.map_or_else(|| "-".to_string(), |a| a.to_string());

            let response = inner.call(req).await?;

            let status = response.status();
            let status_code = status.as_u16();

            // strftime format: %d/%b/%Y:%H:%M:%S %z
            let now: DateTime<Utc> = Utc::now();
            let timestamp = format!("[{}]", now.format("%d/%b/%Y:%H:%M:%S %z"));

            let level = if status.is_server_error() {
                Level::Error
            } else if status.is_client_error() {
                Level::Warn
            } else {
                Level::Info
            };

            // host ident authuser timestamp request-line status bytes
            log!(target:&service_name, level, "{host} - - {timestamp} {method} {uri} {version} {status_code} -");

            Ok(response)
        })
    }
}
