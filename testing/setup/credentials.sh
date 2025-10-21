#!/bin/sh
set -e


CREDS_DIR="/shared/credentials"
mkdir -p "$CREDS_DIR/cln"
mkdir -p "$CREDS_DIR/lnd"

CLN_PUBKEY=$(docker exec cln-regtest lightning-cli --regtest getinfo | jq -r ".id")
echo "$CLN_PUBKEY" > "$CREDS_DIR/cln/node_id"

docker cp cln-regtest:/root/.lightning/regtest/ca.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client-key.pem "$CREDS_DIR/cln/"

LND_PUBKEY=$(docker exec lnd-regtest lncli --network=regtest getinfo | jq -r ".identity_pubkey")
echo "$LND_PUBKEY" > "$CREDS_DIR/lnd/node_id"

docker cp lnd-regtest:/root/.lnd/tls.cert "$CREDS_DIR/lnd/"
docker cp lnd-regtest:/root/.lnd/data/chain/bitcoin/regtest/admin.macaroon "$CREDS_DIR/lnd/"

chmod -R 644 "$CREDS_DIR"
chmod -R +X "$CREDS_DIR"

echo ""
echo "=== CREDENTIALS SUMMARY ==="
echo "CLN:"
echo "  - Node ID: $(cat $CREDS_DIR/cln/node_id)"
echo "  - ca.pem: $CREDS_DIR/cln/ca.pem"
echo "  - client.pem: $CREDS_DIR/cln/client.pem"
echo "  - client-key.pem: $CREDS_DIR/cln/client-key.pem"
echo ""
echo "LND:"
echo "  - Node ID: $(cat $CREDS_DIR/lnd/node_id)"
echo "  - tls.cert: $CREDS_DIR/lnd/tls.cert"
echo "  - admin.macaroon: $CREDS_DIR/lnd/admin.macaroon"

cd /shared
tar -czf credentials.tar.gz credentials/
