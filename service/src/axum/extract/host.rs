use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum_extra::extract::Host;
use log::warn;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct AllowedHosts(pub HashSet<String>);

#[derive(Clone)]
pub struct ValidatedHost(pub String);

impl<S> FromRequestParts<S> for ValidatedHost
where
    S: Send + Sync,
    AllowedHosts: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Host(hostname) = match Host::from_request_parts(parts, state).await {
            Ok(h) => h,
            Err(_) => {
                return Err(StatusCode::BAD_REQUEST);
            }
        };

        let domain = hostname.split(':').next().unwrap_or(&hostname).to_string();

        let allowed_hosts = AllowedHosts::from_ref(state);

        if allowed_hosts.0.is_empty() {
            warn!("host allow list is empty, trusting unvalidated host {domain}",);
        }

        if !allowed_hosts.0.is_empty() && !allowed_hosts.0.contains(&domain) {
            return Err(StatusCode::BAD_REQUEST);
        }

        Ok(ValidatedHost(hostname))
    }
}
