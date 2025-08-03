#!/bin/sh
set -e

echo "Waiting for bitcoind to start..."
while ! docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 getblockchaininfo 2>/dev/null; do
  sleep 1
done

echo "bitcoind is ready. Checking if blocks need to be generated..."
BLOCK_COUNT=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 getblockcount)
echo "Current block count: $BLOCK_COUNT"

if [ "$BLOCK_COUNT" -lt 200 ]; then
  BLOCKS_TO_GENERATE=$((200 - BLOCK_COUNT))
  echo "Generating $BLOCKS_TO_GENERATE blocks..."
  
  # Create temporary wallet for initial block generation
  docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 createwallet 'temp_wallet' 2>/dev/null || true
  
  # Get address from temp wallet
  ADDRESS=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=temp_wallet getnewaddress)
  
  # Generate blocks
  docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress $BLOCKS_TO_GENERATE $ADDRESS
  
  # Unload temp wallet
  docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 unloadwallet 'temp_wallet' 2>/dev/null || true
  
  echo "Generated $BLOCKS_TO_GENERATE blocks. New block count: $(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 getblockcount)"
else
  echo "Already have $BLOCK_COUNT blocks, no generation needed."
fi

echo "Bitcoin initialization complete."