# Testing

Docker-based regtest environment for testing with Lightning Network nodes (CLN, LND), Bitcoin Core, databases, and an OpenTelemetry tracing stack (OTEL Collector + Jaeger).

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

## Tracing Stack

The compose bundle ships an OTLP-gRPC collector fronting a Jaeger v2 backend so tests can exercise real trace export and query spans back out.

- **OTEL Collector** (`otel-collector`, port `OTEL_GRPC_PORT`, default `4317`) — OTLP gRPC receiver that requires TLS, mTLS client-cert verification, **and** a bearer token on every request. Forwards traces to Jaeger.
- **Jaeger** (`jaeger`, UI at `http://localhost:${JAEGER_QUERY_PORT}`, default `16686`) — receives spans over OTLP gRPC on `JAEGER_OTLP_GRPC_PORT`.

### Credentials

`testing/setup/credentials.sh` mints a fresh CA on every setup run, then signs a server leaf cert (for the collector) and a client leaf cert (for exporters) off it, plus a random bearer token. Everything is served from the credentials-server alongside the LN/DB material:

- `credentials/otel-collector/ca.pem` — CA to trust when exporting to the collector, and to validate the client cert on the receiver side.
- `credentials/otel-collector/cert.pem` + `key.pem` — server-side TLS material (fetched by the collector at startup, not needed by clients).
- `credentials/otel-collector/client-cert.pem` + `client-key.pem` — client-side mTLS material every exporter must present.
- `credentials/otel-collector/token` — bearer token required on every OTLP request.

The server cert's SANs cover `localhost`, `docker`, and `otel-collector`, so the same cert verifies from host tests, DinD, and in-network containers.

### Rust helper

`testing::credentials::otel::OtelCredentials` downloads the tarball and hands back an `OtelCollector` with `grpc_endpoint`, `jaeger_query_endpoint`, `ca_cert_path`, `bearer_token_path`, `client_cert_path`, and `client_key_path` — wire these into an OTLP exporter to emit spans, and hit `jaeger_query_endpoint` to assert on them.


