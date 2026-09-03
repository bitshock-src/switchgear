@integration @otlp-metrics @otlp-compliance @offer-store
Feature: OTLP metrics for the offer database store
  As a platform maintainer
  I want every database call the offer service makes to be measured
  So that a slow query or a rising constraint-failure rate is visible
  on a dashboard, and so that an uninstrumented call site fails a test
  rather than leaving a silent gap

  # Scope: this feature covers `db.client.operation.duration` for the
  # twelve call sites in `components/src/offer/db.rs`. The pipeline is
  # the one `otlp-metrics.feature` describes end to end — a `tracing`
  # event on the `swgr::metrics` target, the `MetricsLayer` bridge in
  # the offer service's dispatch, the `SdkMeterProvider`, OTLP gRPC to
  # the containerised collector, and the collector's InfluxDB exporter.
  # What differs is the driver: the `swgr` CLI against a server whose
  # stores are SQLite, so one scenario walks every call site including
  # the two the foreign keys reject.
  #
  # Attributes on each row:
  #
  #   | attribute               | scope    | value                     |
  #   | db.system.name          | store    | sqlite                    |
  #   | db.namespace            | store    | the SQLite file stem      |
  #   | db.collection.name      | site     | the table the call is for |
  #   | swgr.operation          | site     | the call site             |
  #   | error.type              | failure  | the class of failure      |
  #   | db.response.status_code | failure  | the driver's own code     |
  #
  # `error.type` is a class, not a detail: the convention asks for a value
  # that is predictable and low-cardinality. Constraint violations are
  # classified by sea-orm's own `DbErr::sql_err`, which is portable across
  # SQLite, MySQL and Postgres, giving `unique_constraint` and
  # `foreign_key_constraint`; anything else falls back to
  # `connection_acquire`, `connection`, `statement`, `conversion`, or the
  # registry's `_OTHER`. The precise identifier goes in
  # `db.response.status_code`, which stays backend-specific.
  #
  # `swgr.operation` is not from the semantic-convention registry — it
  # names the call site inside this workspace, under the `swgr.` prefix
  # so it can never collide with a registry attribute. `server.address`
  # and `server.port` are absent throughout: SQLite has no server.
  # Conditional attributes are absent rather than empty, so a success
  # row carries no `error.type` column value at all.
  #
  # The recording rule: the metric records the round trip, not the
  # store's reading of it. `get_offer` mapping an empty result to
  # `Ok(None)`, `post_offer` mapping a unique violation to `Ok(None)`,
  # and `delete_offer` mapping a miss to `Ok(false)` are store
  # semantics the metric does not see — all three are successes here.
  # `put_offer` and `put_metadata` each make two round trips, so each
  # contributes two `swgr.operation` values.
  #
  # Isolation and flush work as in `otlp-metrics.feature`: a random
  # `test_identity` resource attribute scopes the run, and SIGTERM
  # drains the meter provider, so the query happens after the child has
  # exited with code 0. `count` is asserted as at least 1, never
  # exactly 1.
  #
  # Out of scope: the MySQL and PostgreSQL variants of this scenario,
  # which are a one-line change via `ctx.set_offer_store_database_uri`;
  # and the offer store's HTTP implementation, whose write surface no
  # service calls.

  Background:
    Given a valid configuration file exists
    And the server is not already running
    And the OTel collector and InfluxDB containers are running
    And the swgr CLI is available
    And the server is configured with SQLite persistence
    And the child server is stamped with a unique test_identity resource attribute

  # -----------------------------------------------------------------
  # One scenario for all twelve call sites: they share a server, a
  # store and a test_identity, and the two failures need the rows the
  # successes create.
  # -----------------------------------------------------------------

  @otlp-metrics-offer-store-operations
  Scenario: Every offer store operation reaches InfluxDB
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer get" for the offer ID
    Then the command should succeed
    When I run "swgr offer get" with no parameters
    Then the command should succeed
    When I run "swgr offer metadata get" for the metadata ID
    Then the command should succeed
    When I run "swgr offer metadata get" with no parameters
    Then the command should succeed
    Given updated offer JSON exists
    When I run "swgr offer put" for the offer ID
    Then the command should succeed
    Given updated offer metadata JSON exists
    When I run "swgr offer metadata put" for the metadata ID
    Then the command should succeed
    When I run "swgr offer get" for a non-existent offer ID
    Then the command should fail
    # The store still succeeded: the empty result becomes `Ok(None)` and
    # only the CLI treats it as an error.
    Given an offer JSON with a non-existent metadata ID exists
    When I run "swgr offer post" with offer JSON
    Then the command should fail
    And a user error message should be shown
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer metadata delete" for metadata an offer references
    Then the command should fail
    And a user error message should be shown
    When I run "swgr offer delete" for the offer ID
    Then the command should succeed
    When I run "swgr offer metadata delete" for the metadata ID
    Then the command should succeed
    When I send a SIGTERM signal to the server process
    # SIGTERM drains the meter provider: the PeriodicReader's own
    # interval is 60s, so shutdown is what puts the run's datapoints on
    # the wire.
    Then the server should exit with code 0
    And InfluxDB should have a "db.client.operation.duration" row for this test_identity under service.name "swgr.offer" for each operation:
      | swgr.operation      | db.collection.name |
      | get_offer           | offer_record       |
      | get_offers          | offer_record       |
      | post_offer          | offer_record       |
      | put_offer_upsert    | offer_record       |
      | put_offer_fetch     | offer_record       |
      | delete_offer        | offer_record       |
      | get_metadata        | offer_metadata     |
      | get_all_metadata    | offer_metadata     |
      | post_metadata       | offer_metadata     |
      | put_metadata_upsert | offer_metadata     |
      | put_metadata_fetch  | offer_metadata     |
      | delete_metadata     | offer_metadata     |
    And each row's "db.system.name" should be "sqlite"
    And each row should carry no "error.type"
    And each row should be a well-formed histogram
    And InfluxDB should have a failed "db.client.operation.duration" row for each rejection:
      | swgr.operation  | error.type             | db.response.status_code     |
      | post_offer      | foreign_key_constraint | 787 (CONSTRAINT_FOREIGNKEY) |
      | delete_metadata | statement              | 1811 (CONSTRAINT_TRIGGER)   |
    # One error.type for both: it is the class of failure, which the
    # convention asks to keep predictable and low-cardinality. The precise
    # identifier is db.response.status_code, the domain-specific attribute
    # the convention pairs with error.type.
    And no ECS log record should carry the metrics target or a metric field

    # `get_offer` appears twice in this scenario — once for the offer
    # that exists and once for the one that does not — which is why the
    # histogram assertion is `count >= 1` rather than `count == 1`.
