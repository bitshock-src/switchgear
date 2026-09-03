@integration @otlp-metrics @otlp-compliance
Feature: OTLP metrics export
  As a platform maintainer
  I want swgr's metric events to reach a metrics sink through the OTLP
  pipeline with the resource and datapoint attributes intact
  So that dashboards and alerts can be built on them, and so that a
  regression in the metrics bridge fails a test rather than silently
  emptying a dashboard

  # Scope: this feature covers the whole metrics path end to end —
  # `switchgear_metrics::histogram!("<name>", …)` at the call site, which
  # expands to a `tracing` event on the `swgr::metrics` target carrying a
  # `histogram.<name>` field; the `MetricsLayer` bridge in the service
  # dispatch; the `SdkMeterProvider` + `PeriodicReader`; OTLP gRPC export to
  # the containerised OTel collector; and the collector's InfluxDB exporter.
  # The assertion is a SQL query against InfluxDB, so it proves the metric
  # is queryable, not merely that bytes left the process.
  #
  # Three instruments are emitted, all histograms of milliseconds:
  #
  #   | metric                          | call sites                        |
  #   | db.client.operation.duration    | offer/db.rs, discovery/db.rs      |
  #   | http.client.request.duration    | offer/http.rs, discovery/http.rs  |
  #   | rpc.client.call.duration        | pool/{cln,lnd}/grpc/client.rs     |
  #
  # Their attributes follow the OpenTelemetry semantic conventions, with one
  # addition: `swgr.operation` names the call site inside this workspace and
  # is not from the registry. Conditional attributes are absent rather than
  # empty — `error.type` appears only on a failure, and a SQLite store has
  # no server address or port at all.
  #
  # This feature covers the LNURL service's read path: its gRPC calls to a
  # Lightning node, and the requests its stores make when those stores are
  # HTTP or database-backed. The database stores' own call sites — every
  # `swgr.operation` in `offer/db.rs` and `discovery/db.rs`, driven by the
  # CLI — live in `otlp-offer-store-metrics.feature` and
  # `otlp-discovery-store-metrics.feature`.
  #
  # Each assertion has exactly one home. `rpc.client.call.duration` belongs
  # to the two gRPC scenarios below, which exist to cover both node types'
  # call sites; the HTTP- and database-store scenarios run the same invoice
  # flow but do not re-assert it, because the step that requests the invoice
  # already fails the scenario if no invoice comes back.
  #
  # The same scenarios assert the other side of the partition: the log layer
  # excludes the metrics target, so the child's ECS stderr carries no metric
  # record — no `swgr::metrics` as `event.module`, and no metric-prefixed
  # field.
  #
  # The collector writes with the `telegraf-prometheus-v1` schema: one
  # measurement per metric name, with resource, scope and datapoint
  # attributes as tags and the histogram body (`count`, `sum`, `min`,
  # `max`, and one column per bucket boundary) as fields. The bucket
  # boundaries are the SDK's defaults, which is why the call sites record
  # milliseconds — seconds-valued data would collapse into the first two
  # buckets.
  #
  # `count` is asserted as at least 1, never exactly 1: a single LNURL flow
  # fetches its offer twice, and the balancer's retry refreshes discovery
  # once per attempt.
  #
  # Isolation: InfluxDB is shared by every test and every previous run, so
  # each scenario stamps its children with a random
  # `OTEL_RESOURCE_ATTRIBUTES=test_identity=<hex>` and selects on it. The
  # tag is set on the children only — the test process exports nothing.
  #
  # Flush: the `PeriodicReader`'s export interval is 60s, far longer than a
  # scenario. SIGTERM drains the meter provider, so shutdown is what puts
  # the run's datapoints on the wire — the query happens after the children
  # have exited with code 0.
  #
  # Out of scope: OTLP metric wire shape (there is no metrics equivalent of
  # the `otlp_spans.rs` snapshots), exporter retry behaviour, and the
  # `otlp.metrics` disabled/`OTEL_METRICS_EXPORTER=none` paths. Also out of
  # scope: metrics from the CLI, which runs without a metrics layer; the
  # HTTP stores' write surface, which no service calls; and the balancer's
  # background refresh loop, which runs outside any service dispatch and so
  # reaches no subscriber.

  Background:
    Given a valid configuration file exists
    And the server is not already running
    And the OTel collector and InfluxDB containers are running
    And the child servers are stamped with a unique test_identity resource attribute

  # -----------------------------------------------------------------
  # rpc.client.call.duration — one scenario per Lightning backend,
  # because `rpc.method` is the wire path of that node's own service.
  # -----------------------------------------------------------------

  @otlp-metrics-grpc-invoice-request-cln
  Scenario: CLN gRPC invoice request timing reaches InfluxDB
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
    And InfluxDB should have a "rpc.client.call.duration" row for this test_identity
    And the row's "service.name" should be "swgr.lnurl"
    And the row's "rpc.method" should be "cln.Node/Invoice"
    And the row should have no "error.type"
    And the row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field

  @otlp-metrics-grpc-invoice-request-lnd
  Scenario: LND gRPC invoice request timing reaches InfluxDB
    Given the single payee has an LND lightning node available
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
    And InfluxDB should have a "rpc.client.call.duration" row for this test_identity
    And the row's "service.name" should be "swgr.lnurl"
    And the row's "rpc.method" should be "lnrpc.Lightning/AddInvoice"
    And the row should have no "error.type"
    And the row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field

  # -----------------------------------------------------------------
  # http.client.request.duration — only the LNURL service has HTTP
  # stores, and only its read path calls them.
  # -----------------------------------------------------------------

  @otlp-metrics-http-store-reads
  Scenario: HTTP store reads reach InfluxDB
    Given the single payee has a CLN lightning node available
    And server 1 serves the offers and discovery services over memory stores
    And server 2 runs only the lnurl service against server 1 over HTTP
    When I start server 1 with offers and discovery services
    And I start server 2 with only lnurl service
    Then server 1 should have offers and discovery services listening
    And server 2 should have only lnurl service listening
    When the single payee creates an offer for their lightning node on server 1
    And the single payee registers their lightning node as a backend on server 1
    And the payer requests the LNURL offer from the single payee on server 2
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then all servers should exit with code 0
    And InfluxDB should have a "http.client.request.duration" row for this test_identity with method "GET" and url.template "/offers/{partition}/{id}" under service.name "swgr.lnurl"
    And InfluxDB should have a "http.client.request.duration" row for this test_identity with method "GET" and url.template "/discovery" under service.name "swgr.lnurl"
    And both rows should carry no "error.type"
    And each row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field

  @otlp-metrics-dead-offer-store
  Scenario: A dead offer store records a connect error
    Given the lnurl server's offer and discovery store URLs point at a dead port
    When I start server 2 with only lnurl service
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And InfluxDB should have a "http.client.request.duration" row for this test_identity with method "GET", url.template "/offers/{partition}/{id}" and "error.type" "connect" under service.name "swgr.lnurl"
    And the row should carry no "http.response.status_code"
    And the row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field
    # The discovery store's connect failure is not reachable from this
    # topology: every request that would refresh the balancer's backends
    # goes through the invoice handler, which fetches the offer first and so
    # never gets past the dead offer store. The next scenario covers it.

  @otlp-metrics-dead-discovery-store
  Scenario: A dead discovery store records a connect error
    Given the single payee has a CLN lightning node available
    And server 1 serves the offers and discovery services over memory stores
    And server 2 runs only the lnurl service, with its offer store on server 1 and its discovery store URL on a dead port
    When I start server 1 with offers and discovery services
    And I start server 2 with only lnurl service
    Then server 1 should have offers and discovery services listening
    And server 2 should have only lnurl service listening
    When the single payee creates an offer for their lightning node on server 1
    And the payer requests the LNURL offer from the single payee on server 2
    And the payer requests an invoice for 100 sats using the single payee's callback URL, expecting failure
    And I send a SIGTERM signal to the server process
    Then all servers should exit with code 0
    And InfluxDB should have a "http.client.request.duration" row for this test_identity with method "GET", url.template "/discovery" and "error.type" "connect" under service.name "swgr.lnurl"
    And the row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field

  # -----------------------------------------------------------------
  # The LNURL read path over database stores. This configuration runs
  # all three services in one process against one pair of SQLite files,
  # which is the only place `service.name` attribution is under real
  # pressure: the setup steps and the LNURL request path share a store,
  # and each row must still be attributed to the dispatch that made the
  # call. One write per other service is enough to show that — the
  # per-service call-site inventories live in their own features.
  # -----------------------------------------------------------------

  @otlp-metrics-db-store-reads
  Scenario: Database store reads reach InfluxDB
    Given the server is configured with SQLite persistence
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And InfluxDB should have a "db.client.operation.duration" row for this test_identity for each of "get_offer", "get_all_etag" and "get_all_backends" under service.name "swgr.lnurl"
    And InfluxDB should have a "db.client.operation.duration" row for "post_offer" under service.name "swgr.offer"
    And InfluxDB should have a "db.client.operation.duration" row for "post" under service.name "swgr.discovery"
    And each row's "db.system.name" should be "sqlite"
    And each row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field
