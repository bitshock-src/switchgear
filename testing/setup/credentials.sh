#!/bin/sh
set -e


CREDS_DIR="/shared/credentials"
mkdir -p "$CREDS_DIR/cln"
mkdir -p "$CREDS_DIR/lnd"
mkdir -p "$CREDS_DIR/postgres"
mkdir -p "$CREDS_DIR/mysql"

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

chmod -R 644 "$CREDS_DIR"
chmod -R +X "$CREDS_DIR"

cd /shared

echo "=== CREDENTIALS ==="
tar cvzf credentials.tar.gz credentials/
