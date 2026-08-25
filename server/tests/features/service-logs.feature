@integration @service-logs
Feature: Service logs — expected ECS field values and server behavior
  As an operator
  I want the server to emit ECS-formatted log lines with the expected values on every request path
  So that dashboards, alerts, and error correlation work end-to-end

  # Scope: this feature covers WHAT the server emits (service.name,
  # method, path, status, level, and the four ECS Categorization Fields
  # event.kind / event.category / event.type / event.outcome) plus the
  # SERVER BEHAVIOR that drives each shape (success paths, 4xx client
  # errors, 5xx upstream errors, extractor rejections, auth failures).
  #
  # Out of scope: ECS spec compliance — key existence, ordering, wire
  # shape stability — lives in `ecs-logs.feature`. OTLP topology and
  # span-metadata compliance lives in `otlp-spans.feature`.
  #
  # Every ECS log line is expected to carry the categorization tuple:
  #   - event.kind      = "event"
  #   - event.category  = ["api"] for swgr.discovery / swgr.offer
  #                       ["web"] for swgr.lnurl
  #   - event.type      = ["access"] on the RequestLogger INFO line
  #                       ["error"] on the CrudError / LnUrlPayServiceError
  #                                 WARN or ERROR line
  #   - event.outcome   = "success" for 2xx/3xx access lines
  #                       "failure" for 4xx/5xx access lines and every
  #                                 error emitter record

  Background:
    Given a valid configuration file exists
    And the server is not already running

  # -----------------------------------------------------------------
  # Success paths — INFO access records with event.outcome = success
  # -----------------------------------------------------------------

  @service-health-logs
  Scenario: Health-check requests are logged for every service
    Given the log level is set to "info" in the configuration
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain health check requests for all services

  @service-operations-logs
  Scenario: Service operation requests are logged (backend, offer, invoice)
    Given the log level is set to "info" in the configuration
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the system waits for backend readiness
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain backend registration requests
    And the server logs should contain offer retrieval requests
    And the server logs should contain invoice generation requests

  @invoice-log-trace-correlation
  Scenario: Successful invoice request correlates ECS log to Jaeger trace
    # RequestLogger emits an ECS INFO record for the invoice call carrying
    # trace.id and span.id populated from the active OTel span, and
    # Jaeger has a matching root span for the same (trace.id, span.id).
    # Also confirms the fmt-layer filter suppresses the RequestLogger
    # summary event so it does NOT duplicate as an OTLP span event.
    Given the log level is set to "info" in the configuration
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats using the single payee's callback URL
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the invoice generation request
    And the invoice ECS access line's trace.id and span.id match a Jaeger root span for "invoice" on service "swgr.lnurl"
    And the Jaeger trace contains no "request" span event

  # -----------------------------------------------------------------
  # Client errors (4xx) — WARN pairs with event.outcome = failure
  # -----------------------------------------------------------------

  @error-logging
  Scenario: 4xx errors log matched INFO/WARN pairs across services
    Given the log level is set to "warn" in the configuration
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer from a non-existent partition
    And I request an invoice for a non-existent offer
    And I try to get a missing backend by public key
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain 404 error responses across all services
    And the server logs should contain invalid offer error responses
    And the server logs should contain the invalid backend GET error

  @invoice-bad-amount-error
  Scenario: Invoice with out-of-range amount logs a 400 WARN pair on lnurl
    # amount=1 msat is below the offer's min_sendable → LnUrlPayServiceError
    # emits a WARN pair on swgr.lnurl (event.category=web).
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 1 msat expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the invoice bad-amount error

  @extractor-uuid-rejection
  Scenario: Malformed UUID on the invoice path is logged as a 404 WARN pair
    # The UuidParam extractor rejects "not-a-uuid" before any handler runs.
    # RequestLogger still emits the access line; the rejection produces a
    # WARN error line on swgr.lnurl.
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request /offers/default/not-a-uuid on the lnurl service
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And a WARN ECS record exists for method "GET" path prefix "/offers/default/" status 404 on the lnurl service

  @extractor-query-rejection
  Scenario: Non-numeric ?amount is logged as a 400 WARN pair
    # The Query<Amount> extractor rejects "amount=abc" — 400 WARN on lnurl.
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an invoice with a non-numeric amount on the lnurl service
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And a WARN ECS record exists for method "GET" path prefix "/offers/default/" status 400 on the lnurl service

  @extractor-json-rejection
  Scenario: Malformed JSON body on POST /discovery is logged as a 400 WARN pair
    # Bearer passes; axum's Json extractor then fails to parse "{not-json".
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the discovery service should be listening on the configured port
    When I post a malformed JSON body to /discovery with a valid bearer
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And a WARN ECS record exists for method "POST" path "/discovery" status 400 on the discovery service

  @unauthorized-backend-error
  Scenario: Unauthenticated POST /discovery is logged as a 401 WARN pair
    # CrudError::unauthorized on the auth middleware — 401 WARN on discovery
    # with event.category=api.
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the discovery service should be listening on the configured port
    When I post to /discovery without a bearer token
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the unauthorized backend error

  @backend-conflict-error
  Scenario: Duplicate backend registration is logged as a 409 WARN pair
    # CrudError::conflict on the second POST /discovery for the same
    # public_key — 409 WARN on swgr.discovery. Also verifies ECS↔OTLP
    # correlation on this error path.
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the discovery service should be listening on the configured port
    When the single payee registers their lightning node as a backend
    And the single payee registers their lightning node as a backend a second time
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the backend conflict error
    And the 409 ECS record correlates with a Jaeger root span for "post_backend" on service "swgr.discovery"

  @offers-bad-request
  Scenario: Over-limit /offers count is logged as a 400 WARN pair on the offer service
    # memory-basic.yaml sets max-page-size=100; asking for count=10_000
    # trips OfferCrudError::bad → CrudError::bad on swgr.offer.
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the offers service should be listening on the configured port
    When I request /offers/default with count 10000 on the offer service
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the offers bad-request error

  @partition-reject-error
  Scenario: Non-existent partition on lnurl is logged as a 404 WARN pair
    # LnUrlPayServiceError::not_found emitted by the partitions middleware —
    # 404 WARN on swgr.lnurl.
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request an offer from a non-existent partition
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And a WARN ECS record exists for method "GET" path prefix "/offers/non-existent-partition/" status 404 on the lnurl service

  # -----------------------------------------------------------------
  # Server errors (5xx) — ERROR pairs with event.outcome = failure
  # -----------------------------------------------------------------

  @invoice-no-backend-error
  Scenario: Invoice with no backend logs a 502 ERROR pair on lnurl
    # Offer exists but no backend registered → PingoraLnError::no_available_nodes
    # → 502 ERROR on swgr.lnurl.
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the invoice no-backend error

  @invoice-unreachable-backend-error
  Scenario: Invoice with an unreachable backend logs a 502 ERROR pair after retry exhaustion
    # Backend registered but its grpc URL points at a dead port. Health
    # checks never pass; balancer retries and gives up — 502 ERROR on lnurl.
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    And the single payee registers a backend with an unreachable grpc URL
    And the payer requests the LNURL offer from the single payee
    And the payer requests an invoice for 100 sats expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the invoice no-backend error

  @offer-upstream-error
  Scenario: Dead HTTP offer-store URL logs a 502 ERROR pair on the lnurl offer path
    # Standalone-lnurl config, HTTP offer store pointed at a dead port.
    # HttpOfferStore.get transport failure → CrudError chain → 502 ERROR
    # on swgr.lnurl with event.category=web.
    Given a standalone lnurl configuration
    And the HTTP offer store URL is redirected to a dead port
    And the HTTP discovery store URL is redirected to a dead port
    When I start only the lnurl service
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I request any offer from the lnurl service expecting failure
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain the offer upstream error

  # -----------------------------------------------------------------
  # Cross-service value locks for the four ECS Categorization Fields
  # -----------------------------------------------------------------

  @event-categorization-per-service
  Scenario: event.category differs per service on access records
    # /health access lines exercise all three services without any
    # downstream dependency. swgr.discovery and swgr.offer emit
    # event.category=["api"]; swgr.lnurl emits event.category=["web"].
    # Also asserts event.kind=event, event.type=["access"],
    # event.outcome="success" universally on the 2xx path.
    Given the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And every /health access record on the lnurl service carries event.category "web", event.kind "event", event.type "access", event.outcome "success"
    And every /health access record on the discovery service carries event.category "api", event.kind "event", event.type "access", event.outcome "success"
    And every /health access record on the offer service carries event.category "api", event.kind "event", event.type "access", event.outcome "success"
    And no /health access record on the lnurl service carries event.category "api"
    And no /health access record on the discovery service carries event.category "web"

  @event-outcome-failure-on-errors
  Scenario: event.outcome is "failure" on both error emitters
    # CrudError (401 on POST /discovery) and LnUrlPayServiceError (404 on
    # GET /offers/non-existent-partition) each emit a WARN pair; both
    # pairs must carry event.outcome="failure".
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    When I post to /discovery without a bearer token
    And I request an offer from a non-existent partition
    And I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And exactly one 401 WARN pair on the discovery service carries event.category "api" and event.outcome "failure"
    And exactly one 404 WARN pair on the lnurl service carries event.category "web" and event.outcome "failure"
