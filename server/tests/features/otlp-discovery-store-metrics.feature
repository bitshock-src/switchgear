@integration @otlp-metrics @otlp-compliance @discovery-store
Feature: OTLP metrics for the discovery database store
  As a platform maintainer
  I want every database call the discovery service makes to be measured
  So that a slow backend lookup is visible on a dashboard, and so that
  an uninstrumented call site fails a test rather than leaving a silent
  gap

  # Scope: this feature covers `db.client.operation.duration` for the
  # seven call sites in `components/src/discovery/db.rs`. It is the
  # mirror of `otlp-offer-store-metrics.feature` — same pipeline, same
  # SQLite-backed server, same `swgr` CLI driving it — over the
  # discovery service's own store.
  #
  # Attributes on each row:
  #
  #   | attribute          | scope | value                          |
  #   | db.system.name     | store | sqlite                         |
  #   | db.namespace       | store | the SQLite file stem           |
  #   | db.collection.name | site  | the table the call is about    |
  #   | swgr.operation     | site  | the call site                  |
  #
  # `swgr.operation` is not from the semantic-convention registry — it
  # names the call site inside this workspace, under the `swgr.` prefix
  # so it can never collide with a registry attribute. `server.address`
  # and `server.port` are absent throughout: SQLite has no server.
  #
  # Two call sites are worth naming. `get_all` makes two round trips —
  # an etag lookup and, when the caller's etag is stale, the backend
  # fetch — so it contributes both `get_all_etag` and
  # `get_all_backends`. And the four writes are each a single
  # `DatabaseConnection::transaction` spanning `discovery_backend` and
  # `discovery_backend_etag`: the transaction is the unit measured, its
  # own Result decides the outcome, and `db.collection.name` names the
  # table the call is about while the etag bump is incidental.
  #
  # `patch` covers three CLI commands: `discovery patch`, `discovery
  # disable` and `discovery enable` all reach the same call site.
  #
  # Isolation and flush work as in `otlp-metrics.feature`: a random
  # `test_identity` resource attribute scopes the run, and SIGTERM
  # drains the meter provider, so the query happens after the child has
  # exited with code 0. `count` is asserted as at least 1, never
  # exactly 1 — `patch` alone runs three times here.
  #
  # No `error.type` row: the discovery schema has no foreign keys, so
  # no call this API surface can make will fail the transaction. The
  # `error.type` mapping itself is unit-tested against every `DbErr`
  # variant in `components/src/metrics.rs`, and its end-to-end path is
  # covered by `otlp-offer-store-metrics.feature`, whose foreign keys
  # do reject.
  #
  # Out of scope: the MySQL and PostgreSQL variants of this scenario,
  # which are a one-line change via
  # `ctx.set_discovery_store_database_uri`; and the discovery store's
  # HTTP implementation, whose write surface no service calls.

  Background:
    Given a valid configuration file exists
    And the server is not already running
    And the OTel collector and InfluxDB containers are running
    And the swgr CLI is available
    And the server is configured with SQLite persistence
    And the child server is stamped with a unique test_identity resource attribute

  # -----------------------------------------------------------------
  # One scenario for all seven call sites: they share a server, a store
  # and a test_identity, and the reads need the rows the writes create.
  # -----------------------------------------------------------------

  @otlp-metrics-discovery-store-operations
  Scenario: Every discovery store operation reaches InfluxDB
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery get" for the backend public key
    Then the command should succeed
    When I run "swgr discovery ls"
    Then the command should succeed
    When I run "swgr discovery get-all"
    Then the command should succeed
    Given updated backend JSON exists
    When I run "swgr discovery put" for the backend public key
    Then the command should succeed
    Given backend patch JSON exists
    When I run "swgr discovery patch" for the backend public key
    Then the command should succeed
    When I run "swgr discovery disable" for the backend public key
    Then the command should succeed
    When I run "swgr discovery enable" for the backend public key
    Then the command should succeed
    When I run "swgr discovery get" for a non-existent backend public key
    Then the command should fail
    # The store still succeeded: the empty result becomes `Ok(None)` and
    # only the CLI treats it as an error.
    When I run "swgr discovery delete" for the backend public key
    Then the command should succeed
    When I send a SIGTERM signal to the server process
    # SIGTERM drains the meter provider: the PeriodicReader's own
    # interval is 60s, so shutdown is what puts the run's datapoints on
    # the wire.
    Then the server should exit with code 0
    And InfluxDB should have a "db.client.operation.duration" row for this test_identity under service.name "swgr.discovery" for each operation:
      | swgr.operation   | db.collection.name      | driven by                       |
      | get              | discovery_backend       | discovery get                   |
      | get_all_etag     | discovery_backend_etag  | discovery ls, discovery get-all |
      | get_all_backends | discovery_backend       | discovery ls, discovery get-all |
      | post             | discovery_backend       | discovery post                  |
      | put              | discovery_backend       | discovery put                   |
      | patch            | discovery_backend       | patch, disable, enable          |
      | delete           | discovery_backend       | discovery delete                |
    And each row's "db.system.name" should be "sqlite"
    And each row should carry no "error.type"
    And each row should be a well-formed histogram
    And no ECS log record should carry the metrics target or a metric field
