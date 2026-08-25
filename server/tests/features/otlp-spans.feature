@integration @otlp-spans @otlp-compliance
Feature: OTLP span-tree compliance
  As a platform maintainer
  I want to lock in the OTLP span tree swgr puts on the wire
  So that changes to trace topology, span attributes, or status
  propagation surface as test failures before shipping

  # Scope: this feature covers what swgr emits on the OTLP wire — the
  # full ExportTracesServiceRequest span tree produced by the
  # `tracing-opentelemetry` layer + `axum-tracing-opentelemetry`. A
  # per-test in-process OTLP gRPC collector (plain HTTP/2) receives
  # spans, reassembles the parent/child tree by parent_span_id, and
  # emits a deterministically-sorted JSON-lines file that `insta`
  # snapshots. Jaeger's v1 query API mutates the shape (drops per-span
  # Resource, adds `otel.scope.*` tags, renames Event.name → `event`,
  # etc.), so these tests are the only ones that catch OTLP-shape drift.
  #
  # What is asserted per snapshot:
  #   - Root-span names on the wire (only the whitelisted names for
  #     each scenario are emitted; other ambient spans are dropped).
  #   - Parent/child tree topology, sorted deterministically.
  #   - Every OTLP `Span` metadata field is present with a type
  #     placeholder — `name` is emitted literally; `kind`, `trace_id`,
  #     `span_id`, timestamps, drop counts are `[<type>]`;
  #     `parent_span_id` is `null` on root spans, `[bytes]` otherwise;
  #     `status.code` is the OTel-spec enum name (STATUS_CODE_UNSET /
  #     _OK / _ERROR) because its value drives RED metrics.
  #   - Every attribute KEY is preserved verbatim; attribute values
  #     type-redacted.
  #
  # Out of scope: expected VALUES on the wire (per-request trace ids,
  # timings, message contents) — those are inherently per-run.
  # ECS-side log compliance lives in `ecs-logs.feature`; ECS field
  # values live in `service-logs.feature`.
  #
  # On a snapshot diff: run `cargo insta review`, audit that the change
  # is intentional (usually a `tracing-opentelemetry` /
  # `opentelemetry-otlp` bump or an intentional emit-shape change), then
  # accept.

  Background:
    Given a valid configuration file exists
    And the server is not already running
    And an in-process OTLP gRPC collector is spawned on 127.0.0.1
    And the child server is configured to export OTLP to the collector

  # -----------------------------------------------------------------
  # Success path — root span with STATUS_CODE_UNSET (2xx)
  # -----------------------------------------------------------------

  @otlp-shape-success-invoice
  Scenario: Success invoice OTLP span tree matches the golden snapshot
    # Whitelisted root span: "GET /offers/{partition}/{id}/invoice".
    # Full waterfall: axum HTTP root → route span → invoice handler →
    # pingora balancer get_invoice → pool client → gRPC transport, plus
    # the offer-lookup subtree. Root status is STATUS_CODE_UNSET (2xx).
    # Snapshot: features__otlp_spans__otlp_spans__success_invoice.snap
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
    # SIGTERM flushes the OTLP BatchExporter's shutdown path, so the
    # collector sees every buffered span before the log is written.
    Then the server should exit with code 0
    And the collector's OTLP span tree for root "GET /offers/{partition}/{id}/invoice" matches the insta snapshot "otlp_spans__success_invoice"

  # -----------------------------------------------------------------
  # 4xx path — root span with STATUS_CODE_UNSET (client error)
  # -----------------------------------------------------------------

  @otlp-shape-error-404
  Scenario: 4xx errors leave the root span with STATUS_CODE_UNSET
    # Per OTel HTTP semconv, 4xx responses MUST leave the root span
    # status UNSET (client fault, not server fault). Two 404 requests
    # against the whitelisted root "GET /offers/{partition}/{id}" produce
    # two sibling root spans, both STATUS_CODE_UNSET.
    # Snapshot: features__otlp_spans__otlp_spans__error_get_offer_404.snap
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer from a non-existent partition
    And I request an invoice for a non-existent offer
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the collector's OTLP span tree for root "GET /offers/{partition}/{id}" matches the insta snapshot "otlp_spans__error_get_offer_404"

  # -----------------------------------------------------------------
  # 5xx path — root span with STATUS_CODE_ERROR (server error)
  # -----------------------------------------------------------------

  @otlp-shape-dead-store-502
  Scenario: 5xx errors set the root span to STATUS_CODE_ERROR
    # Per OTel HTTP semconv, 5xx responses MUST set the root span
    # `Span.status.code = ERROR` on the SERVER root span. Wiring is
    # done automatically by `axum-tracing-opentelemetry`'s
    # `OtelAxumLayer` via `update_span_from_response`. This scenario
    # uses a dead HTTP offer store so reqwest transport fails on the
    # first connect — no pingora retries, one clean root subtree with
    # `status.code = STATUS_CODE_ERROR`.
    # Non-root child spans stay STATUS_CODE_UNSET by design: error
    # rates for internal layers are not meaningful; the request as a
    # whole failed once at the root.
    # Snapshot: features__otlp_spans__otlp_spans__dead_offer_store_502.snap
    Given a standalone lnurl configuration
    And the HTTP offer store URL is redirected to a dead port
    And the HTTP discovery store URL is redirected to a dead port
    When I start only the lnurl service
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request any offer from the lnurl service expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the collector's OTLP span tree for root "GET /offers/{partition}/{id}" matches the insta snapshot "otlp_spans__dead_offer_store_502"
    And the root span "GET /offers/{partition}/{id}" carries status.code "STATUS_CODE_ERROR"
    And every non-root span carries status.code "STATUS_CODE_UNSET"
