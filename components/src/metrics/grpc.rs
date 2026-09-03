use opentelemetry_semantic_conventions::attribute::{
    ERROR_TYPE, RPC_METHOD, RPC_RESPONSE_STATUS_CODE, RPC_SYSTEM_NAME, SERVER_ADDRESS, SERVER_PORT,
};
use std::time::Duration;

pub(crate) fn record_rpc_call<T>(
    elapsed: Duration,
    method: &'static str,
    address: &str,
    port: u16,
    result: &Result<tonic::Response<T>, tonic::Status>,
) {
    let code = match result {
        Ok(_) => tonic::Code::Ok,
        Err(status) => status.code(),
    };
    let status_code = grpc_status_name(code);
    switchgear_metrics::histogram!(
        "rpc.client.call.duration",
        elapsed,
        RPC_SYSTEM_NAME => "grpc",
        RPC_METHOD => method,
        SERVER_ADDRESS => address,
        SERVER_PORT => port,
        RPC_RESPONSE_STATUS_CODE => status_code,
        ERROR_TYPE => (code != tonic::Code::Ok).then_some(status_code),
    );
}

fn grpc_status_name(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "OK",
        tonic::Code::Cancelled => "CANCELLED",
        tonic::Code::Unknown => "UNKNOWN",
        tonic::Code::InvalidArgument => "INVALID_ARGUMENT",
        tonic::Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        tonic::Code::NotFound => "NOT_FOUND",
        tonic::Code::AlreadyExists => "ALREADY_EXISTS",
        tonic::Code::PermissionDenied => "PERMISSION_DENIED",
        tonic::Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        tonic::Code::FailedPrecondition => "FAILED_PRECONDITION",
        tonic::Code::Aborted => "ABORTED",
        tonic::Code::OutOfRange => "OUT_OF_RANGE",
        tonic::Code::Unimplemented => "UNIMPLEMENTED",
        tonic::Code::Internal => "INTERNAL",
        tonic::Code::Unavailable => "UNAVAILABLE",
        tonic::Code::DataLoss => "DATA_LOSS",
        tonic::Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_status_names_are_canonical() {
        assert_eq!(grpc_status_name(tonic::Code::Ok), "OK");
        assert_eq!(grpc_status_name(tonic::Code::Unavailable), "UNAVAILABLE");
        assert_eq!(
            grpc_status_name(tonic::Code::DeadlineExceeded),
            "DEADLINE_EXCEEDED"
        );
        assert_eq!(
            grpc_status_name(tonic::Code::InvalidArgument),
            "INVALID_ARGUMENT"
        );
        assert_eq!(
            grpc_status_name(tonic::Code::Unauthenticated),
            "UNAUTHENTICATED"
        );
    }
}
