//! Shared ECS-content assertion for the extractor-rejection tests.
//!
//! Reduced from an ECS+Jaeger composite to a pure ECS-content check —
//! OTLP-shape auditing is now in `otlp_spans.rs` (via an in-process
//! collector + `insta` snapshots) and ECS↔OTLP correlation lives in the
//! two designated correlation tests in `service_logs.rs`. This helper
//! stays because the extractor tests share the same "assert an ECS
//! access-log line with this shape exists" boilerplate.

use crate::bail_log;
use crate::common::context::Service;
use crate::common::context::global::GlobalContext;
use crate::common::helpers::{EcsRequestFilter, count_ecs_requests, read_active_stderr_lines};
use anyhow::Result;

#[derive(Copy, Clone, Debug)]
pub enum EcsLevel {
    Info,
    Warn,
    Error,
}

impl EcsLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            EcsLevel::Info => "INFO",
            EcsLevel::Warn => "WARN",
            EcsLevel::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExpectedErrorContext<'a> {
    pub service: Service,
    pub method: &'a str,
    pub path_prefix: &'a str,
    pub status: u16,
    pub level: EcsLevel,
}

fn service_name(service: Service) -> &'static str {
    match service {
        Service::LnUrl => "lnurl",
        Service::Discovery => "discovery",
        Service::Offer => "offer",
    }
}

// `event.category` value the emitters pair with each service. Wired in
// `server/src/di/inject/injectors/service/{discovery,offer,balance}.rs`
// and cross-cited from the ECS audit § 5.1.4.
fn event_category(service: Service) -> &'static str {
    match service {
        Service::LnUrl => "web",
        Service::Discovery | Service::Offer => "api",
    }
}

/// Assert that at least one ECS request-log line matching `expected` was
/// emitted on the active server's stderr. Every match also carries the
/// full ECS categorization tuple (`event.kind`, `event.category`,
/// `event.type`, `event.outcome`) — enforced so an emitter regression that
/// drops one of the four values fails the test rather than silently
/// omitting the field.
pub async fn step_and_expected_error_should_be_logged(
    ctx: &mut GlobalContext,
    expected: ExpectedErrorContext<'_>,
) -> Result<()> {
    let stderr_lines = read_active_stderr_lines(ctx)?;

    let category = event_category(expected.service);
    let filter = EcsRequestFilter {
        service: Some(service_name(expected.service)),
        method: Some(expected.method),
        path_prefix: Some(expected.path_prefix),
        status: Some(expected.status as u64),
        level: Some(expected.level.as_str()),
        event_kind: Some("event"),
        event_category: Some(category),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    let count = count_ecs_requests(&stderr_lines, &filter);
    if count < 1 {
        bail_log!(
            "❌ ECS log line missing: service=swgr.{} method={} path_prefix={} status={} level={} event.category={category}",
            service_name(expected.service),
            expected.method,
            expected.path_prefix,
            expected.status,
            expected.level.as_str()
        );
    }
    Ok(())
}
