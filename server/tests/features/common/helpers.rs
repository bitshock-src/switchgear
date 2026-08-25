use crate::common::context::Service;
use crate::common::context::global::GlobalContext;
use crate::common::context::pay::{OfferRequest, PayeeContext};
use crate::{anyhow_log, bail_log};
use anyhow::Result;
use lightning_invoice::Bolt11Invoice;
use std::fmt::Display;
use std::str::FromStr;
use switchgear_service_api::discovery::HttpDiscoveryBackendClient;
use switchgear_service_api::lnurl::LnUrlOffer;
use switchgear_service_api::offer::HttpOfferClient;
use uuid::Uuid;

fn get_required_with_error<T>(option: Option<T>, error_msg: impl FnOnce() -> String) -> Result<T> {
    option.ok_or_else(|| anyhow_log!(error_msg()))
}

fn validate_expectation<T: PartialEq + Display>(
    actual: T,
    expected: T,
    value_name: &str,
) -> Result<()> {
    if actual != expected {
        bail_log!("Expected {} {}, got", value_name, expected,)
    } else {
        Ok(())
    }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct EcsRequestFilter<'a> {
    pub service: Option<&'a str>,
    pub method: Option<&'a str>,
    pub path_exact: Option<&'a str>,
    pub path_prefix: Option<&'a str>,
    pub query_prefix: Option<&'a str>,
    pub status: Option<u64>,
    pub level: Option<&'a str>,
    pub event_kind: Option<&'a str>,
    // Matched by array-contains against the JSON `event.category` array.
    pub event_category: Option<&'a str>,
    // Matched by array-contains against the JSON `event.type` array.
    pub event_type: Option<&'a str>,
    pub event_outcome: Option<&'a str>,
}

pub fn parse_ecs_line(line: &str) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    v.is_object().then_some(v)
}

pub fn matches_ecs_request(line: &str, f: &EcsRequestFilter) -> bool {
    let Some(v) = parse_ecs_line(line) else {
        return false;
    };
    let get_str = |k: &str| v.get(k).and_then(|x| x.as_str());
    let get_u64 = |k: &str| v.get(k).and_then(|x| x.as_u64());
    let array_contains = |k: &str, needle: &str| {
        v.get(k)
            .and_then(|x| x.as_array())
            .is_some_and(|arr| arr.iter().any(|e| e.as_str() == Some(needle)))
    };

    if let Some(s) = f.service {
        let expected_name = format!("swgr.{s}");
        if get_str("service.name") != Some(expected_name.as_str()) {
            return false;
        }
    }
    if let Some(m) = f.method
        && get_str("http.request.method") != Some(m)
    {
        return false;
    }
    if let Some(p) = f.path_exact
        && get_str("url.path") != Some(p)
    {
        return false;
    }
    if let Some(p) = f.path_prefix {
        let Some(path) = get_str("url.path") else {
            return false;
        };
        if !path.starts_with(p) {
            return false;
        }
    }
    if let Some(q) = f.query_prefix {
        let Some(query) = get_str("url.query") else {
            return false;
        };
        if !query.starts_with(q) {
            return false;
        }
    }
    if let Some(s) = f.status
        && get_u64("http.response.status_code") != Some(s)
    {
        return false;
    }
    if let Some(l) = f.level
        && get_str("log.level") != Some(l)
    {
        return false;
    }
    if let Some(k) = f.event_kind
        && get_str("event.kind") != Some(k)
    {
        return false;
    }
    if let Some(c) = f.event_category
        && !array_contains("event.category", c)
    {
        return false;
    }
    if let Some(t) = f.event_type
        && !array_contains("event.type", t)
    {
        return false;
    }
    if let Some(o) = f.event_outcome
        && get_str("event.outcome") != Some(o)
    {
        return false;
    }
    true
}

pub fn count_ecs_requests(lines: &[String], f: &EcsRequestFilter) -> usize {
    match f.level {
        // WARN/ERROR emissions produce a matched pair of records: an INFO
        // access line from RequestLogger + an error line from the emitter.
        // Count the pairs by taking the min of the two filtered counts.
        //
        // - RequestLogger records don't carry event.* fields (see
        //   ecs-log-field-audit.md § 5), so the access-side filter uses
        //   only transport-shape fields (service, method, path, query,
        //   status).
        // - The error-side filter carries the caller's event.* expectations
        //   plus `event_type = "error"`. HTTP request-shape fields (method,
        //   path, query) only appear on the access record, so they're dropped
        //   from the error-side filter.
        Some("WARN") | Some("ERROR") => {
            let access = EcsRequestFilter {
                service: f.service,
                method: f.method,
                path_exact: f.path_exact,
                path_prefix: f.path_prefix,
                query_prefix: f.query_prefix,
                status: f.status,
                level: Some("INFO"),
                ..Default::default()
            };
            let error = EcsRequestFilter {
                method: None,
                path_exact: None,
                path_prefix: None,
                query_prefix: None,
                event_type: Some("error"),
                ..*f
            };
            let access_count = lines
                .iter()
                .filter(|line| matches_ecs_request(line, &access))
                .count();
            let error_count = lines
                .iter()
                .filter(|line| matches_ecs_request(line, &error))
                .count();
            access_count.min(error_count)
        }
        _ => lines
            .iter()
            .filter(|line| matches_ecs_request(line, f))
            .count(),
    }
}

/// Snapshot the active server's stderr as a `Vec<String>`. Bails if the
/// buffer mutex is poisoned or the buffer is empty (the latter almost
/// always indicates a test-harness bug rather than legitimate silence).
pub fn read_active_stderr_lines(ctx: &GlobalContext) -> Result<Vec<String>> {
    let lines = ctx
        .get_active_stderr_buffer()?
        .lock()
        .map_err(|_| anyhow_log!("Failed to lock stderr buffer"))?
        .clone();
    if lines.is_empty() {
        bail_log!("No stderr logs captured");
    }
    Ok(lines)
}

fn update_offer_request_invoice(
    ctx: &mut GlobalContext,
    payee_name: &str,
    offer_key: &str,
    invoice: Option<String>,
) -> Result<()> {
    let offer_request = ctx
        .get_offer_request_mut(payee_name, offer_key)
        .ok_or_else(|| {
            anyhow_log!(
                "Offer request '{}' not found for payee '{}'",
                offer_key,
                payee_name
            )
        })?;
    offer_request.received_invoice = invoice;
    Ok(())
}

fn get_offer_request_with_error<'a>(
    ctx: &'a GlobalContext,
    payee_name: &str,
    offer_key: &str,
) -> Result<&'a OfferRequest> {
    ctx.get_offer_request(payee_name, offer_key).ok_or_else(|| {
        anyhow_log!(
            "Offer request '{}' not found for payee '{}'",
            offer_key,
            payee_name
        )
    })
}

fn create_context_error_message(
    context_type: &str,
    context_id: &str,
    item_type: Option<&str>,
) -> String {
    match item_type {
        Some(item) => format!("No {item} found for {context_type} {context_id}"),
        None => format!("No {context_type} {context_id} found"),
    }
}

pub async fn check_health_endpoint_for_service_url(
    ctx: &mut GlobalContext,
    service: Service,
) -> Result<bool> {
    match service {
        Service::Discovery => {
            let discovery_client = ctx.get_active_discovery_client()?;
            match discovery_client.health().await {
                Ok(()) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        Service::Offer => {
            let offer_client = ctx.get_active_offer_client()?;
            match offer_client.health().await {
                Ok(()) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        Service::LnUrl => {
            // LnUrl service health check using LnUrlTestClient
            let client = ctx.get_active_lnurl_client()?;
            match client.health().await {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
    }
}

pub async fn check_all_services_health(
    ctx: &mut GlobalContext,
    services: Vec<Service>,
) -> Result<bool> {
    for service in services {
        if !check_health_endpoint_for_service_url(ctx, service).await? {
            return Ok(false);
        }
    }

    Ok(true)
}

pub async fn verify_exit_code(ctx: &mut GlobalContext, expected_zero: bool) -> Result<()> {
    let exit_code = ctx.wait_active_exit_code()?;

    let is_success = exit_code == 0;
    validate_expectation(
        is_success,
        expected_zero,
        &format!(
            "{} exit code",
            if expected_zero { "zero" } else { "non-zero" }
        ),
    )
}

fn create_payee_error_message(item_type: &str, payee_id: &str) -> String {
    create_context_error_message("payee", payee_id, Some(item_type))
}

pub fn get_payee_from_context<'a>(
    ctx: &'a GlobalContext,
    payee_id: &str,
) -> Result<&'a PayeeContext> {
    get_required_with_error(ctx.get_payee(payee_id), || {
        create_context_error_message("payee", payee_id, None)
    })
}

pub fn get_payee_from_context_mut<'a>(
    ctx: &'a mut GlobalContext,
    payee_id: &str,
) -> Result<&'a mut PayeeContext> {
    get_required_with_error(ctx.get_payee_mut(payee_id), || {
        create_context_error_message("payee", payee_id, None)
    })
}

pub fn get_offer_request<'a>(
    payee: &'a PayeeContext,
    payee_id: &str,
) -> Result<(&'a String, &'a OfferRequest)> {
    get_required_with_error(payee.offer_requests.iter().next(), || {
        create_payee_error_message("offer requests", payee_id)
    })
}

pub fn get_offer_request_mut<'a>(
    payee: &'a mut PayeeContext,
    payee_id: &str,
) -> Result<(&'a String, &'a mut OfferRequest)> {
    get_required_with_error(payee.offer_requests.iter_mut().next(), || {
        create_payee_error_message("offer requests", payee_id)
    })
}

pub fn get_offer_id_from_request(offer_request: &OfferRequest, payee_id: &str) -> Result<Uuid> {
    get_required_with_error(offer_request.offer_id, || {
        create_payee_error_message("offer ID", payee_id)
    })
}

pub fn get_lnurl_offer_from_request<'a>(
    offer_request: &'a OfferRequest,
    payee_id: &str,
) -> Result<&'a LnUrlOffer> {
    get_required_with_error(offer_request.lnurl_offer.as_ref(), || {
        create_payee_error_message("LNURL offer", payee_id)
    })
}

pub fn get_invoice_from_request<'a>(
    offer_request: &'a OfferRequest,
    payee_id: &str,
) -> Result<&'a str> {
    get_required_with_error(offer_request.received_invoice.as_deref(), || {
        create_payee_error_message("invoice", payee_id)
    })
}

pub fn parse_lightning_invoice(invoice_str: &str) -> Result<Bolt11Invoice> {
    Bolt11Invoice::from_str(invoice_str)
        .map_err(|e| anyhow_log!("Invalid Lightning invoice: {}", e))
}

pub fn validate_invoice_has_amount(invoice: &Bolt11Invoice, expected_msat: u64) -> Result<()> {
    match invoice.amount_milli_satoshis() {
        Some(amount) if amount == expected_msat => Ok(()),
        Some(amount) => bail_log!(
            "Invoice amount mismatch: expected {} msat, got {} msat",
            expected_msat,
            amount
        ),
        None => bail_log!("Invoice does not specify an amount"),
    }
}

pub async fn check_services_listening_status(
    ctx: &mut GlobalContext,
    services: Vec<Service>,
    should_be_listening: bool,
    individual_timeout_ms: u64,
    retry_timeout_secs: u64,
) -> Result<()> {
    use std::time::{Duration, Instant};
    use tokio::time::{sleep as tokio_sleep, timeout};

    let retry_duration = Duration::from_secs(retry_timeout_secs);
    let start = Instant::now();

    while start.elapsed() < retry_duration {
        let mut all_services_match_expectation = true;

        for service in services.clone() {
            let health_check = timeout(
                Duration::from_millis(individual_timeout_ms),
                check_health_endpoint_for_service_url(ctx, service),
            )
            .await;

            let service_is_responding = match health_check {
                Ok(h) => h.unwrap_or(false),
                Err(_) => false,
            };

            if should_be_listening {
                // We expect the service to be listening
                if !service_is_responding {
                    all_services_match_expectation = false;
                    break;
                }
            } else if service_is_responding {
                bail_log!("unexpected response from: {service}",);
            }
        }

        if all_services_match_expectation {
            return Ok(());
        }

        if should_be_listening {
            // Only sleep and retry for positive checks
            tokio_sleep(Duration::from_millis(200)).await;
        } else {
            return Ok(());
        }
    }

    // If we reach here, positive check timed out
    if should_be_listening {
        bail_log!(
            "{:?} failed to start listening on their configured ports within timeout",
            services
        )
    } else {
        // This shouldn't happen for negative checks
        Ok(())
    }
}

pub async fn request_and_validate_invoice_helper(
    ctx: &mut GlobalContext,
    payee_name: &str,
    _service: &Service,
) -> Result<()> {
    // Clear any previous invoice
    update_offer_request_invoice(ctx, payee_name, "multi_backend_offer", None)?;

    // Request invoice
    let offer_request = get_offer_request_with_error(ctx, payee_name, "multi_backend_offer")?;

    if let Some(lnurl_offer) = &offer_request.lnurl_offer {
        let client = ctx.get_active_lnurl_client()?;

        let invoice_response = client.get_invoice(lnurl_offer, 100000).await?; // 100 sats in msat
        let invoice_string = invoice_response.pr;

        // Store the received invoice
        update_offer_request_invoice(
            ctx,
            payee_name,
            "multi_backend_offer",
            Some(invoice_string.clone()),
        )?;

        // Validate the invoice
        let parsed_invoice = parse_lightning_invoice(&invoice_string)?;
        validate_invoice_has_amount(&parsed_invoice, 100000)?;
    } else {
        bail_log!("No callback URL available");
    }

    Ok(())
}

pub async fn verify_single_service_status(
    ctx: &mut GlobalContext,
    services: Vec<Service>,
    should_be_listening: bool,
    timeout_ms: u64,
    retry_timeout_secs: u64,
) -> Result<()> {
    check_services_listening_status(
        ctx,
        services,
        should_be_listening,
        timeout_ms,
        retry_timeout_secs,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_line() -> String {
        r#"{"@timestamp":"t","log.level":"INFO","message":"request","ecs.version":"8.11","service.name":"swgr.lnurl","http.request.method":"GET","http.response.status_code":200,"url.path":"/offers/default/x","event.kind":"event","event.category":["web"],"event.type":["access"],"event.outcome":"success"}"#.to_string()
    }

    fn error_line() -> String {
        r#"{"@timestamp":"t","log.level":"WARN","message":"nope","ecs.version":"8.11","service.name":"swgr.lnurl","http.response.status_code":404,"event.kind":"event","event.category":["web"],"event.type":["error"],"event.outcome":"failure"}"#.to_string()
    }

    #[test]
    fn matches_event_kind_scalar() {
        let f = EcsRequestFilter {
            event_kind: Some("event"),
            ..Default::default()
        };
        assert!(matches_ecs_request(&access_line(), &f));

        let f = EcsRequestFilter {
            event_kind: Some("metric"),
            ..Default::default()
        };
        assert!(!matches_ecs_request(&access_line(), &f));
    }

    #[test]
    fn matches_event_category_by_array_contains() {
        let f = EcsRequestFilter {
            event_category: Some("web"),
            ..Default::default()
        };
        assert!(matches_ecs_request(&access_line(), &f));

        let f = EcsRequestFilter {
            event_category: Some("api"),
            ..Default::default()
        };
        assert!(!matches_ecs_request(&access_line(), &f));
    }

    #[test]
    fn matches_event_type_by_array_contains() {
        let f = EcsRequestFilter {
            event_type: Some("access"),
            ..Default::default()
        };
        assert!(matches_ecs_request(&access_line(), &f));

        let f = EcsRequestFilter {
            event_type: Some("error"),
            ..Default::default()
        };
        assert!(!matches_ecs_request(&access_line(), &f));
        assert!(matches_ecs_request(&error_line(), &f));
    }

    #[test]
    fn matches_event_outcome_scalar() {
        let f = EcsRequestFilter {
            event_outcome: Some("success"),
            ..Default::default()
        };
        assert!(matches_ecs_request(&access_line(), &f));

        let f = EcsRequestFilter {
            event_outcome: Some("failure"),
            ..Default::default()
        };
        assert!(!matches_ecs_request(&access_line(), &f));
        assert!(matches_ecs_request(&error_line(), &f));
    }

    #[test]
    fn count_warn_pair_uses_new_event_fields() {
        // A single 4xx request produces one INFO access line (RequestLogger,
        // no event.* fields) and one WARN error line (from the emitter,
        // with the full event.* tuple). `count_ecs_requests(min)` = 1.
        // The access-side filter uses only transport-shape fields;
        // event.* expectations apply only to the error record.
        let f = EcsRequestFilter {
            service: Some("lnurl"),
            status: Some(404),
            level: Some("WARN"),
            event_category: Some("web"),
            ..Default::default()
        };
        let access = r#"{"@timestamp":"t","log.level":"INFO","message":"request","ecs.version":"8.11","service.name":"swgr.lnurl","http.request.method":"GET","http.response.status_code":404,"url.path":"/x"}"#.to_string();
        let warn = r#"{"@timestamp":"t","log.level":"WARN","message":"nope","ecs.version":"8.11","service.name":"swgr.lnurl","http.response.status_code":404,"event.kind":"event","event.category":["web"],"event.type":["error"],"event.outcome":"failure"}"#.to_string();
        assert_eq!(count_ecs_requests(&[access, warn], &f), 1);
    }
}
