pub(crate) mod db;
pub(crate) mod grpc;
pub(crate) mod http;

#[cfg(test)]
mod tests {
    use opentelemetry_semantic_conventions::metric;

    #[test]
    fn metric_names_match_the_semantic_conventions() {
        assert_eq!(
            "db.client.operation.duration",
            metric::DB_CLIENT_OPERATION_DURATION
        );
        assert_eq!(
            "http.client.request.duration",
            metric::HTTP_CLIENT_REQUEST_DURATION
        );
        assert_eq!("rpc.client.call.duration", metric::RPC_CLIENT_CALL_DURATION);
    }
}
