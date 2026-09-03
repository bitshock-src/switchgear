use opentelemetry_semantic_conventions::attribute::{
    ERROR_TYPE, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, SERVER_ADDRESS, SERVER_PORT,
    URL_TEMPLATE,
};
use std::time::Duration;

pub(crate) fn record_http_request(
    elapsed: Duration,
    method: &'static str,
    template: &'static str,
    address: &str,
    port: u16,
    result: &Result<reqwest::Response, reqwest::Error>,
) {
    let status = match result {
        Ok(response) => Some(response.status()),
        Err(e) => e.status(),
    };
    let failed = status.is_some_and(|s| s.is_client_error() || s.is_server_error());
    let error_type = if failed {
        status.as_ref().map(reqwest::StatusCode::as_str)
    } else {
        result.as_ref().err().map(classify_request_error)
    };
    switchgear_metrics::histogram!(
        "http.client.request.duration",
        elapsed,
        HTTP_REQUEST_METHOD => method,
        SERVER_ADDRESS => address,
        SERVER_PORT => port,
        URL_TEMPLATE => template,
        HTTP_RESPONSE_STATUS_CODE => status.map(|s| s.as_u16()),
        ERROR_TYPE => error_type,
    );
}

fn classify_request_error(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_decode() {
        "decode"
    } else {
        "request"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn install_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[tokio::test]
    async fn request_errors_are_classified_by_predicate() {
        install_crypto_provider();

        let connect = reqwest::Client::new()
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .expect_err("connection refused");
        assert!(connect.is_connect(), "{connect:?}");
        assert_eq!(classify_request_error(&connect), "connect");

        let (silent, _silent_task) = spawn_tcp(None).await;
        let timeout = reqwest::Client::builder()
            .timeout(Duration::from_millis(250))
            .build()
            .expect("client")
            .get(format!("http://{silent}/"))
            .send()
            .await
            .expect_err("timeout");
        assert!(timeout.is_timeout(), "{timeout:?}");
        assert_eq!(classify_request_error(&timeout), "timeout");

        let (server, _server_task) = spawn_tcp(Some(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 9\r\n\r\nnot json!",
        ))
        .await;
        let decode = reqwest::Client::new()
            .get(format!("http://{server}/"))
            .send()
            .await
            .expect("response")
            .json::<serde_json::Value>()
            .await
            .expect_err("decode");
        assert!(decode.is_decode(), "{decode:?}");
        assert_eq!(classify_request_error(&decode), "decode");

        let other = reqwest::Client::new()
            .get("ftp://127.0.0.1/")
            .send()
            .await
            .expect_err("unsupported scheme");
        assert!(
            !other.is_timeout() && !other.is_connect() && !other.is_decode(),
            "{other:?}"
        );
        assert_eq!(classify_request_error(&other), "request");
    }

    #[tokio::test]
    async fn a_failing_response_carries_its_status_as_the_error_type() {
        install_crypto_provider();

        let (server, _task) = spawn_tcp(Some(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
        ))
        .await;
        let response = reqwest::Client::new()
            .get(format!("http://{server}/"))
            .send()
            .await
            .expect("response");

        let status = response.status();
        assert!(status.is_server_error());
        assert_eq!(status.as_str(), "503");
    }

    async fn spawn_tcp(
        response: Option<&'static str>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let task = tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            match response {
                Some(response) => {
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.flush().await;
                }
                None => std::future::pending::<()>().await,
            }
        });
        (address, task)
    }
}
