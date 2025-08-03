use axum::extract::FromRef;
use axum::http::header;
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_forwarded_header::ForwardedHeader;
use std::convert::Infallible;

#[derive(Debug, Clone)]
pub struct Scheme(pub String);

impl<S> FromRequestParts<S> for Scheme
where
    S: Send + Sync,
    Scheme: FromRef<S>,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(proto) = parts
            .headers
            .get(header::FORWARDED)
            .and_then(|h| ForwardedHeader::try_from(h).ok())
            .and_then(|h| h.proto)
        {
            return Ok(Scheme(proto));
        }
        if let Some(proto) = parts
            .headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
        {
            return Ok(Scheme(proto.to_string()));
        }

        Ok(Scheme::from_ref(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRef;
    use axum::http::{HeaderMap, HeaderValue, Request};

    #[derive(Clone)]
    struct TestState {
        default_scheme: Scheme,
    }

    impl FromRef<TestState> for Scheme {
        fn from_ref(state: &TestState) -> Self {
            state.default_scheme.clone()
        }
    }

    async fn extract_scheme(headers: HeaderMap, state: TestState) -> Scheme {
        let request = Request::builder().uri("/").body(()).unwrap();

        let (mut parts, _) = request.into_parts();
        parts.headers = headers;

        Scheme::from_request_parts(&mut parts, &state)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_forwarded_header_with_proto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::FORWARDED,
            HeaderValue::from_static("proto=https;host=example.com"),
        );

        let state = TestState {
            default_scheme: Scheme("http".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "https");
    }

    #[tokio::test]
    async fn test_x_forwarded_proto_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        let state = TestState {
            default_scheme: Scheme("http".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "https");
    }

    #[tokio::test]
    async fn test_both_headers_forwarded_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(header::FORWARDED, HeaderValue::from_static("proto=wss"));
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        let state = TestState {
            default_scheme: Scheme("http".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "wss");
    }

    #[tokio::test]
    async fn test_fallback_to_state() {
        let headers = HeaderMap::new();

        let state = TestState {
            default_scheme: Scheme("https".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "https");
    }

    #[tokio::test]
    async fn test_invalid_forwarded_header_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::FORWARDED,
            HeaderValue::from_static("invalid-forwarded-header"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        let state = TestState {
            default_scheme: Scheme("http".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "https");
    }

    #[tokio::test]
    async fn test_forwarded_header_without_proto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::FORWARDED,
            HeaderValue::from_static("for=192.0.2.60;host=example.com"),
        );

        let state = TestState {
            default_scheme: Scheme("https".to_string()),
        };

        let scheme = extract_scheme(headers, state).await;
        assert_eq!(scheme.0, "https");
    }
}
