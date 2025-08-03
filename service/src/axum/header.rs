use axum::http::{header, HeaderMap, HeaderValue};

pub fn no_cache_headers() -> HeaderMap {
    HeaderMap::from_iter(vec![
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate"),
        ),
        (
            header::EXPIRES,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        ),
        (header::PRAGMA, HeaderValue::from_static("no-cache")),
    ])
}
