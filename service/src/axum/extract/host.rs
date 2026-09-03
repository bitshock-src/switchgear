use axum::extract::{FromRef, FromRequestParts};
use axum::http::StatusCode;
use axum::http::header::{FORWARDED, HOST, HeaderMap};
use axum::http::request::Parts;
use axum::http::uri::Authority;
use std::collections::HashSet;

const X_FORWARDED_HOST_HEADER_KEY: &str = "X-Forwarded-Host";

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
        let hostname = extract_host(parts).ok_or(StatusCode::BAD_REQUEST)?;

        let domain = hostname.split(':').next().unwrap_or(&hostname).to_string();

        let allowed_hosts = AllowedHosts::from_ref(state);

        if !allowed_hosts.0.contains(&domain) {
            return Err(StatusCode::BAD_REQUEST);
        }

        Ok(ValidatedHost(hostname))
    }
}

fn extract_host(parts: &Parts) -> Option<String> {
    if let Some(host) = parse_forwarded(&parts.headers) {
        return Some(host.to_owned());
    }

    if let Some(host) = parts
        .headers
        .get(X_FORWARDED_HOST_HEADER_KEY)
        .and_then(|host| host.to_str().ok())
    {
        return Some(host.to_owned());
    }

    if let Some(host) = parts.headers.get(HOST).and_then(|host| host.to_str().ok()) {
        return Some(host.to_owned());
    }

    if let Some(authority) = parts.uri.authority() {
        return Some(parse_authority(authority).to_owned());
    }

    None
}

fn parse_forwarded(headers: &HeaderMap) -> Option<&str> {
    let forwarded_values = headers.get(FORWARDED)?.to_str().ok()?;

    let first_value = forwarded_values.split(',').next()?;

    first_value.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("host")
            .then(|| value.trim().trim_matches('"'))
    })
}

fn parse_authority(auth: &Authority) -> &str {
    auth.as_str()
        .rsplit('@')
        .next()
        .unwrap_or_else(|| auth.as_str())
}
