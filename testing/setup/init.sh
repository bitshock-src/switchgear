#!/bin/sh
set -e

echo "Starting initialization process..."

echo "Step 1: Bitcoin initialization"
/bitcoin.sh

echo "Step 2: Lightning Network setup"
/ln.sh

echo "Step 3: Copying credentials"
# If no arguments provided, use default addresses for the regtest environment
if [ $# -eq 0 ]; then
    /credentials.sh --cln-address "127.0.0.1:9736" --lnd-address "127.0.0.1:10009"
else
    /credentials.sh "$@"
fi

echo "Initialization complete!"