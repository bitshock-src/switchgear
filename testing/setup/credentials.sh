#!/bin/sh
set -e


CREDS_DIR="/shared/credentials"
mkdir -p "$CREDS_DIR/cln"
mkdir -p "$CREDS_DIR/lnd"
mkdir -p "$CREDS_DIR/postgres"
mkdir -p "$CREDS_DIR/mysql"
mkdir -p "$CREDS_DIR/otel-collector"
mkdir -p "$CREDS_DIR/influxdb"

CLN_PUBKEY=$(docker exec cln-regtest lightning-cli --regtest getinfo | jq -r ".id")
echo "$CLN_PUBKEY" > "$CREDS_DIR/cln/node_id"

docker cp cln-regtest:/root/.lightning/regtest/ca.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client-key.pem "$CREDS_DIR/cln/"

LND_PUBKEY=$(docker exec lnd-regtest lncli --network=regtest getinfo | jq -r ".identity_pubkey")
echo "$LND_PUBKEY" > "$CREDS_DIR/lnd/node_id"

docker cp lnd-regtest:/root/.lnd/tls.cert "$CREDS_DIR/lnd/"
docker cp lnd-regtest:/root/.lnd/data/chain/bitcoin/regtest/admin.macaroon "$CREDS_DIR/lnd/"

docker cp postgres-db:/var/lib/postgresql/server.pem "$CREDS_DIR/postgres/"

docker cp mysql-db:/etc/mysql/certs/server.pem "$CREDS_DIR/mysql/"

# Mint a test CA plus a server (leaf) cert it signs for the otel-collector's
# TLS gRPC receiver. rustls (used by common OTLP exporters) rejects a CA cert
# used as the leaf, so we serve the leaf (cert.pem / key.pem) and exporters
# trust the CA (ca.pem). SANs cover every hostname the collector is reachable
# under (localhost / docker / otel-collector).
OTEL_DIR="$CREDS_DIR/otel-collector"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$OTEL_DIR/ca.key" -out "$OTEL_DIR/ca.pem" \
  -subj "/CN=otel-collector-ca" -addext "basicConstraints=critical,CA:TRUE"
openssl req -newkey rsa:2048 -nodes \
  -keyout "$OTEL_DIR/key.pem" -out "$OTEL_DIR/server.csr" \
  -subj "/CN=otel-collector"
printf "subjectAltName=DNS:localhost,DNS:docker,DNS:otel-collector,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\n" > "$OTEL_DIR/server.ext"
openssl x509 -req -in "$OTEL_DIR/server.csr" -CA "$OTEL_DIR/ca.pem" -CAkey "$OTEL_DIR/ca.key" \
  -CAcreateserial -days 3650 -extfile "$OTEL_DIR/server.ext" \
  -out "$OTEL_DIR/cert.pem"

# Client cert for mTLS. The collector's OTLP receiver enforces both bearer-token
# auth AND client-cert verification (see otel-collector-config.yml), so every
# exporter must present a valid cert signed by ca.pem in addition to the bearer.
openssl req -newkey rsa:2048 -nodes \
  -keyout "$OTEL_DIR/client-key.pem" -out "$OTEL_DIR/client.csr" \
  -subj "/CN=swgr-otel-client"
printf "basicConstraints=critical,CA:FALSE\nextendedKeyUsage=clientAuth\n" \
  > "$OTEL_DIR/client.ext"
openssl x509 -req -in "$OTEL_DIR/client.csr" -CA "$OTEL_DIR/ca.pem" -CAkey "$OTEL_DIR/ca.key" \
  -CAcreateserial -days 3650 -extfile "$OTEL_DIR/client.ext" \
  -out "$OTEL_DIR/client-cert.pem"

rm -f "$OTEL_DIR/ca.key" "$OTEL_DIR/server.csr" "$OTEL_DIR/server.ext" \
      "$OTEL_DIR/client.csr" "$OTEL_DIR/client.ext" "$OTEL_DIR/ca.srl"

# Bearer token the collector's OTLP receiver requires. Shared with tests via
# the credentials tarball.
openssl rand -hex 32 > "$OTEL_DIR/token"

# InfluxDB 3 mints one admin token per data directory and rejects a second
# create. A 409 here means the influxdb container outlived a `setup` recreate;
# recover with `docker compose --env-file ./testing.env down -v` then up.
curl -sS -f -X POST \
  "http://${INFLUXDB_HOSTNAME}:${INFLUXDB_PORT}/api/v3/configure/token/admin" \
  | jq -r '.token' > "$CREDS_DIR/influxdb/token"

chmod -R 644 "$CREDS_DIR"
chmod -R +X "$CREDS_DIR"

cd /shared

echo "=== CREDENTIALS ==="
tar cvzf credentials.tar.gz credentials/
