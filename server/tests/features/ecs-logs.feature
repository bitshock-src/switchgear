@integration @ecs-logs @ecs-compliance
Feature: ECS log-line compliance
  As a platform maintainer
  I want to lock in the ECS wire shape swgr emits on stderr
  So that renamed keys, dropped keys, changed value types, or spec-order
  violations surface as test failures before shipping

  # Scope: this feature covers ECS SPEC COMPLIANCE — key existence,
  # per-record shape, record count, and the ECS-logging spec's field
  # ordering (@timestamp, log.level, message, ecs.version as the first
  # four keys). Assertions are two-sided:
  #
  #   1. Snapshot-based (`insta`) — every ECS field emitted appears in
  #      the snapshot, and any extra unexpected field appears as a diff.
  #      Value type is asserted (`[string]` / `[int]` / `[bool]` / …);
  #      value contents are redacted so run-specific values don't churn.
  #   2. Wire-order-based (`EcsReducer::key_orders`) — the streaming
  #      serde visitor reads each stderr line preserving insertion
  #      order and asserts the first four keys are exactly the
  #      ECS-logging MVP prefix.
  #
  # Out of scope: expected VALUES per emitter (service.name, event.*,
  # http.response.status_code semantics) — those live in
  # `service-logs.feature`. OTLP topology lives in `otlp-spans.feature`.
  #
  # On a snapshot diff: run `cargo insta review`, audit that the change
  # is intentional (usually a `tracing-ecs-formatter` bump, an ECS-spec
  # version bump, or a call-site attribute-set change), then accept.

  Background:
    Given a valid configuration file exists
    And the server is not already running

  # -----------------------------------------------------------------
  # Snapshot compliance — full record shape per emitter
  # -----------------------------------------------------------------

  @ecs-shape-success-invoice
  Scenario: Success invoice ECS shape matches the golden snapshot
    # RequestLogger INFO access record on GET /offers/{partition}/{id}/invoice.
    # Snapshot: features__ecs_logs__ecs_logs__success_invoice.snap
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the reduced stderr for filter service "swgr.lnurl" level "INFO" path prefix "/offers/default/" matches the insta snapshot "ecs_logs__success_invoice"

  @ecs-shape-error-http-request
  Scenario: Error HTTP request emits a paired INFO + WARN record set
    # 404 from LnUrlPayServiceError::not_found on the partitions middleware.
    # Snapshot: features__ecs_logs__ecs_logs__error_http_request.snap
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer from a non-existent partition
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the reduced stderr for filter service "swgr.lnurl" levels "INFO,WARN" path prefix "/offers/non-existent-partition/" error.type "LnUrlPayServiceError" matches the insta snapshot "ecs_logs__error_http_request"

  @ecs-shape-error-log
  Scenario: 5xx upstream error emits a full ERROR triple with stack trace
    # Dead HTTP offer store → LnUrlPayServiceError::boxed_error 502 with
    # error.type, error.message, error.stack_trace. Two INFO access lines
    # accompany (health-check probe + the failing offer request).
    # Snapshot: features__ecs_logs__ecs_logs__error_log.snap
    Given a standalone lnurl configuration
    And the HTTP offer store URL is redirected to a dead port
    And the HTTP discovery store URL is redirected to a dead port
    When I start only the lnurl service
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request any offer from the lnurl service expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the reduced stderr for filter service "swgr.lnurl" levels "ERROR,INFO,WARN" error.type "CrudError,LnUrlPayServiceError" matches the insta snapshot "ecs_logs__error_log"

  # -----------------------------------------------------------------
  # Wire order — ECS-logging spec field ordering
  # -----------------------------------------------------------------
  #
  # ECS-logging spec
  # (https://github.com/elastic/ecs-logging/blob/main/spec/README.md):
  # the ordering of the first three keys must be respected in every
  # ecs-logging library: `@timestamp`, `log.level`, `message`. The
  # fourth key is `ecs.version`, defining the minimum-viable-log MVP.
  # Assertions read the raw stderr line with a streaming serde visitor
  # so the snapshot's lexicographic sort does not mask a wire-order
  # regression.

  @ecs-wire-order-info
  Scenario: RequestLogger INFO access lines respect the ECS-logging spec order
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And every INFO access record on the lnurl service under "/offers/default/" starts with the keys "@timestamp", "log.level", "message", "ecs.version" in that order

  @ecs-wire-order-warn
  Scenario: LnUrlPayServiceError WARN records respect the ECS-logging spec order
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer from a non-existent partition
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And every WARN error record on the lnurl service with error.type "LnUrlPayServiceError" starts with the keys "@timestamp", "log.level", "message", "ecs.version" in that order

  @ecs-wire-order-error
  Scenario: CrudError and LnUrlPayServiceError ERROR records respect the ECS-logging spec order
    Given a standalone lnurl configuration
    And the HTTP offer store URL is redirected to a dead port
    And the HTTP discovery store URL is redirected to a dead port
    When I start only the lnurl service
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request any offer from the lnurl service expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And every ERROR record on the lnurl service with error.type "CrudError" or "LnUrlPayServiceError" starts with the keys "@timestamp", "log.level", "message", "ecs.version" in that order
