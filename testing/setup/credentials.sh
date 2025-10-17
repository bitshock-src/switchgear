#!/bin/sh
set -e

CLN_ADDRESS=""
LND_ADDRESS=""

while [ $# -gt 0 ]; do
    case $1 in
        --cln-address)
            CLN_ADDRESS="$2"
            shift 2
            ;;
        --lnd-address)
            LND_ADDRESS="$2"
            shift 2
            ;;
        *)
            echo "Unknown parameter: $1"
            exit 1
            ;;
    esac
done

if [ -z "$CLN_ADDRESS" ] || [ -z "$LND_ADDRESS" ]; then
    echo "Usage: $0 --cln-address <host:port> --lnd-address <host:port>"
    echo "Example: $0 --cln-address 127.0.0.1:9736 --lnd-address 127.0.0.1:10009"
    exit 1
fi

CREDS_DIR="/shared/credentials"
mkdir -p "$CREDS_DIR/cln"
mkdir -p "$CREDS_DIR/lnd"

CLN_PUBKEY=$(docker exec cln-regtest lightning-cli --regtest getinfo | jq -r ".id")
echo "$CLN_PUBKEY" > "$CREDS_DIR/cln/node_id"

echo "$CLN_ADDRESS" > "$CREDS_DIR/cln/address.txt"

docker cp cln-regtest:/root/.lightning/regtest/ca.pem "$CREDS_DIR/cln/" 2>/dev/null || echo "Warning: Could not copy CLN ca.pem"
docker cp cln-regtest:/root/.lightning/regtest/client.pem "$CREDS_DIR/cln/" 2>/dev/null || echo "Warning: Could not copy CLN client.pem"
docker cp cln-regtest:/root/.lightning/regtest/client-key.pem "$CREDS_DIR/cln/" 2>/dev/null || echo "Warning: Could not copy CLN client-key.pem"

LND_PUBKEY=$(docker exec lnd-regtest lncli --network=regtest getinfo | jq -r ".identity_pubkey")
echo "$LND_PUBKEY" > "$CREDS_DIR/lnd/node_id"

echo "$LND_ADDRESS" > "$CREDS_DIR/lnd/address.txt"

docker cp lnd-regtest:/root/.lnd/tls.cert "$CREDS_DIR/lnd/"
docker cp lnd-regtest:/root/.lnd/data/chain/bitcoin/regtest/admin.macaroon "$CREDS_DIR/lnd/"

chmod -R 644 "$CREDS_DIR"
chmod -R +X "$CREDS_DIR"

echo ""
echo "=== CREDENTIALS SUMMARY ==="
echo "CLN:"
echo "  - Node ID: $(cat $CREDS_DIR/cln/node_id)"
echo "  - Address: $(cat $CREDS_DIR/cln/address.txt)"
echo "  - ca.pem: $CREDS_DIR/cln/ca.pem"
echo "  - client.pem: $CREDS_DIR/cln/client.pem"
echo "  - client-key.pem: $CREDS_DIR/cln/client-key.pem"
echo ""
echo "LND:"
echo "  - Node ID: $(cat $CREDS_DIR/lnd/node_id)"
echo "  - Address: $(cat $CREDS_DIR/lnd/address.txt)"
echo "  - tls.cert: $CREDS_DIR/lnd/tls.cert"
echo "  - admin.macaroon: $CREDS_DIR/lnd/admin.macaroon"