# Testing

Docker-based regtest environment for testing with Lightning Network nodes (CLN, LND), Bitcoin Core, databases, and an OpenTelemetry telemetry stack (OTEL Collector + Jaeger for traces, InfluxDB for metrics).

## Local Testing

1. **Start services:**
   ```bash
   cd testing
   docker compose --env-file ./testing.env up -d --build --wait 
   ```

2. **Copy environment configuration:**
   ```bash
   cp testing/testing.env ./testing.env
   ```

3. **Edit `testing.env` and change all service names to localhost:**

```shell
CLN_HOSTNAME=localhost
CREDENTIALS_SERVER_HOSTNAME=localhost
LND_HOSTNAME=localhost
MYSQL_HOSTNAME=localhost
POSTGRES_HOSTNAME=localhost
OTEL_GRPC_HOSTNAME=localhost
JAEGER_HOSTNAME=localhost
INFLUXDB_HOSTNAME=localhost
```

4. **Run tests:**
   ```bash
   cargo test
   ```

## Docker-in-Docker CI Testing

For running tests inside a container with Docker socket access.

1. **Start services:**
   ```bash
   cd testing
   docker compose --env-file ./testing.env up -d --build --wait 
   ```

2. **Connect container to services network:**
   ```bash
   . testing/testing.env
   docker network connect $SERVICES_NETWORK_NAME $(hostname)
   ```

3. **Copy environment configuration:**
   ```bash
   cp testing/testing.env ./testing.env
   ```

4. **Run tests:**
   ```bash
   cargo test
   ```

## Telemetry Stack

The compose bundle ships an OTLP-gRPC collector fronting a Jaeger v2 backend for traces and an InfluxDB 3 backend for metrics, so tests can exercise real export and query telemetry back out.

- **OTEL Collector** (`otel-collector`, port `OTEL_GRPC_PORT`, default `4317`) — OTLP gRPC receiver that requires TLS, mTLS client-cert verification, **and** a bearer token on every request. Fans out: traces to Jaeger, metrics to InfluxDB.
- **Jaeger** (`jaeger`, UI at `http://localhost:${JAEGER_QUERY_PORT}`, default `16686`) — receives spans over OTLP gRPC on `JAEGER_OTLP_GRPC_PORT`.
- **InfluxDB** (`influxdb`, port `INFLUXDB_PORT`, default `8181`) — InfluxDB 3 Core. The collector writes metrics into database `otel` using the `telegraf-prometheus-v1` schema: one measurement per metric name, with resource, scope, and datapoint attributes as tags. Query it with `POST /api/v3/query_sql`, passing `{"db": "otel", "q": "<SQL>"}` and the admin token as a bearer. The container keeps its data in its own layer, so `down` discards the metrics and the admin token together.

### Credentials

`testing/setup/credentials.sh` mints a fresh CA on every setup run, then signs a server leaf cert (for the collector) and a client leaf cert (for exporters) off it, plus a random bearer token. Everything is served from the credentials-server alongside the LN/DB material:

- `credentials/otel-collector/ca.pem` — CA to trust when exporting to the collector, and to validate the client cert on the receiver side.
- `credentials/otel-collector/cert.pem` + `key.pem` — server-side TLS material (fetched by the collector at startup, not needed by clients).
- `credentials/otel-collector/client-cert.pem` + `client-key.pem` — client-side mTLS material every exporter must present.
- `credentials/otel-collector/token` — bearer token required on every OTLP request.
- `credentials/influxdb/token` — InfluxDB 3 admin token, minted by the setup container against the running `influxdb` service. Required on every query. InfluxDB mints exactly one admin token per data directory and rejects a second create, so if `setup` is recreated while `influxdb` survives, minting fails with a 409 — recover with `docker compose --env-file ./testing.env down -v` then up.

The server cert's SANs cover `localhost`, `docker`, and `otel-collector`, so the same cert verifies from host tests, DinD, and in-network containers.

### Rust helper

`testing::credentials::otel::OtelCredentials` downloads the tarball and hands back an `OtelCollector` with `grpc_endpoint`, `jaeger_query_endpoint`, `ca_cert_path`, `bearer_token_path`, `client_cert_path`, and `client_key_path` — wire these into an OTLP exporter to emit spans, and hit `jaeger_query_endpoint` to assert on them.

`switchgear_testing::credentials::influx::InfluxCredentials` is the metrics counterpart: it hands back an `Influx` with `query_endpoint`, `token`, and `token_path`. Keep the `InfluxCredentials` (and `OtelCredentials`) value bound for as long as you use the paths — each owns the `TempDir` those paths point into.

`switchgear_testing::influx::InfluxClient::new(query_endpoint, token)` queries the metrics database. `query` issues a single `POST /api/v3/query_sql` and returns the rows as `serde_json::Value`; `wait_for_rows` and `wait_for` poll until a row (optionally, a matching row) appears, retrying transport failures, non-2xx responses, and empty results alike — InfluxDB creates databases and tables lazily, so a missing table is a normal transient state. Defaults are database `otel`, a 30s timeout and a 500ms poll interval, each overridable with `with_db` / `with_timeout` / `with_poll_interval`.

`server/tests/features/otlp_metrics.rs` is the live example: it stamps the child with a random `test_identity` resource attribute, exercises an invoice, and queries the histogram back out of InfluxDB.
