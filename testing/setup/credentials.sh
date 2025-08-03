#!/bin/sh
set -e

# Parse command line arguments for external addresses
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

echo "Step 3: Copying Lightning node credentials to mounted volume..."
echo "CLN external address: $CLN_ADDRESS"
echo "LND external address: $LND_ADDRESS"

# Create credentials directory if it doesn't exist
CREDS_DIR="/shared/credentials"
mkdir -p "$CREDS_DIR" 2>/dev/null || echo "Warning: Could not create $CREDS_DIR (this is normal on some systems)"
mkdir -p "$CREDS_DIR/cln" 2>/dev/null || echo "Warning: Could not create $CREDS_DIR/cln (this is normal on some systems)"
mkdir -p "$CREDS_DIR/lnd" 2>/dev/null || echo "Warning: Could not create $CREDS_DIR/lnd (this is normal on some systems)"

echo "Extracting CLN credentials..."

# CLN node id (public key)
CLN_PUBKEY=$(docker exec cln-regtest lightning-cli --regtest getinfo | jq -r ".id")
echo "$CLN_PUBKEY" > "$CREDS_DIR/cln/node_id"
echo "CLN node ID: $CLN_PUBKEY"

# CLN external address
echo "$CLN_ADDRESS" > "$CREDS_DIR/cln/address.txt"
echo "CLN address: $CLN_ADDRESS"

# CLN certificates
docker cp cln-regtest:/root/.lightning/regtest/ca.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client.pem "$CREDS_DIR/cln/"
docker cp cln-regtest:/root/.lightning/regtest/client-key.pem "$CREDS_DIR/cln/"

echo "Extracting LND credentials..."

# LND node id (public key)
LND_PUBKEY=$(docker exec lnd-regtest lncli --network=regtest getinfo | jq -r ".identity_pubkey")
echo "$LND_PUBKEY" > "$CREDS_DIR/lnd/node_id"
echo "LND node ID: $LND_PUBKEY"

# LND external address
echo "$LND_ADDRESS" > "$CREDS_DIR/lnd/address.txt"
echo "LND address: $LND_ADDRESS"

# LND certificates and macaroon
docker cp lnd-regtest:/root/.lnd/tls.cert "$CREDS_DIR/lnd/"
docker cp lnd-regtest:/root/.lnd/data/chain/bitcoin/regtest/admin.macaroon "$CREDS_DIR/lnd/"

echo "Setting appropriate file permissions..."
chmod -R 644 "$CREDS_DIR" 2>/dev/null || echo "Warning: Could not set file permissions (this is normal on some systems)"
chmod -R +X "$CREDS_DIR" 2>/dev/null || echo "Warning: Could not set directory permissions (this is normal on some systems)"

echo "Credentials copied successfully!"
echo "CLN credentials: $CREDS_DIR/cln/"
echo "LND credentials: $CREDS_DIR/lnd/"

# Display summary
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