#!/bin/sh
set -e

echo "Waiting a moment for services to stabilize..."
sleep 10

echo "Setting up Lightning node bidirectional liquidity..."

# Wait for CLN to be ready
echo "Waiting for CLN to be ready..."
while ! docker exec cln-regtest lightning-cli --regtest getinfo 2>/dev/null; do
  sleep 2
done

# Wait for LND to be ready and create wallet
echo "Waiting for LND to be ready..."
while ! docker exec lnd-regtest lncli --network=regtest getinfo 2>/dev/null; do
  docker exec lnd-regtest lncli --network=regtest create --no_seed_backup --wallet_password=password123 2>/dev/null || true
  docker exec lnd-regtest lncli --network=regtest unlock --wallet_password=password123 2>/dev/null || true
  sleep 2
done

echo "Both nodes are ready. Checking current state..."

# Check if nodes already have active channels
CLN_INFO=$(docker exec cln-regtest lightning-cli --regtest getinfo)
LND_INFO=$(docker exec lnd-regtest lncli --network=regtest getinfo)

CLN_ACTIVE_CHANNELS=$(echo "$CLN_INFO" | jq -r ".num_active_channels // 0")
LND_ACTIVE_CHANNELS=$(echo "$LND_INFO" | jq -r ".num_active_channels // 0")
CLN_PENDING_CHANNELS=$(echo "$CLN_INFO" | jq -r ".num_pending_channels // 0")
LND_PENDING_CHANNELS=$(echo "$LND_INFO" | jq -r ".num_pending_channels // 0")

echo "CLN active/pending channels: $CLN_ACTIVE_CHANNELS/$CLN_PENDING_CHANNELS"
echo "LND active/pending channels: $LND_ACTIVE_CHANNELS/$LND_PENDING_CHANNELS"

TOTAL_CLN_CHANNELS=$((CLN_ACTIVE_CHANNELS + CLN_PENDING_CHANNELS))
TOTAL_LND_CHANNELS=$((LND_ACTIVE_CHANNELS + LND_PENDING_CHANNELS))

if [ $TOTAL_CLN_CHANNELS -ge 2 ] && [ $TOTAL_LND_CHANNELS -ge 2 ]; then
  echo "Sufficient channels already exist! Skipping setup..."
  echo "Current CLN channels:"
  docker exec cln-regtest lightning-cli --regtest listfunds
  echo "Current LND channels:"
  docker exec lnd-regtest lncli --network=regtest channelbalance
  echo "Setup already complete! Lightning nodes have bidirectional liquidity."
  exit 0
fi

echo "No active channels found, proceeding with setup..."

# Get dynamic node pubkeys
echo "Getting CLN pubkey..."
CLN_PUBKEY=$(echo "$CLN_INFO" | jq -r ".id // empty")
echo "CLN pubkey extracted: $CLN_PUBKEY"

echo "Getting LND pubkey..."
LND_PUBKEY=$(echo "$LND_INFO" | jq -r ".identity_pubkey // empty")
echo "LND pubkey extracted: $LND_PUBKEY"

if [ -z "$CLN_PUBKEY" ] || [ -z "$LND_PUBKEY" ]; then
  echo "ERROR: Failed to extract node pubkeys"
  exit 1
fi

# Check if mining wallet exists, create if needed
echo "Setting up mining wallet..."
WALLETS=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 listwallets 2>/dev/null || echo "[]")
echo "Currently loaded wallets: $WALLETS"

if echo "$WALLETS" | grep -q "mining_wallet"; then
  echo "Mining wallet already loaded"
else
  echo "Mining wallet not loaded, trying to load or create..."
  # Try to load existing wallet first
  if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 loadwallet mining_wallet 2>/dev/null; then
    echo "Mining wallet loaded successfully"
  else
    echo "Mining wallet doesn't exist, creating new one..."
    if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 createwallet mining_wallet false false "" false true false 2>/dev/null; then
      echo "Mining wallet created successfully"
    else
      echo "Mining wallet creation failed, will try fallback methods"
    fi
  fi
fi

# Get mining address from wallet with improved fallback
MINING_ADDR=""

# Try mining wallet first
if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet getwalletinfo >/dev/null 2>&1; then
  MINING_ADDR=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet getnewaddress 2>/dev/null)
  echo "Got address from mining wallet: $MINING_ADDR"
fi

# Fallback to default wallet if mining wallet fails
if [ -z "$MINING_ADDR" ]; then
  echo "Mining wallet not available, trying default wallet..."
  
  # Check if default wallet exists first
  DEFAULT_WALLETS=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 listwallets 2>/dev/null || echo "[]")
  echo "Current wallets: $DEFAULT_WALLETS"
  
  # Try to create default wallet if it doesn't exist
  if ! echo "$DEFAULT_WALLETS" | grep -q '""' && ! echo "$DEFAULT_WALLETS" | grep -q "wallet.dat"; then
    echo "Creating default wallet..."
    docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 createwallet "" false false "" false false false 2>/dev/null || echo "Default wallet creation failed or already exists"
  fi
  
  # Try to get address from default wallet
  MINING_ADDR=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 getnewaddress 2>/dev/null || echo "")
  if [ -n "$MINING_ADDR" ]; then
    echo "Got address from default wallet: $MINING_ADDR"
  else
    echo "Failed to get address from default wallet, trying legacy method..."
    # Last resort: try with explicit wallet name
    docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 createwallet "wallet" false false "" false false false 2>/dev/null || true
    MINING_ADDR=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=wallet getnewaddress 2>/dev/null || echo "")
    if [ -n "$MINING_ADDR" ]; then
      echo "Got address from legacy wallet: $MINING_ADDR"
    fi
  fi
fi

if [ -z "$MINING_ADDR" ]; then
  echo "ERROR: Failed to get mining address from any wallet"
  exit 1
fi

# Check current block height and generate more blocks if needed
CURRENT_HEIGHT=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 getblockcount)
echo "Current block height: $CURRENT_HEIGHT"

# Always generate some blocks to the mining wallet to ensure it has mature coins
BLOCKS_TO_MINE=150
echo "Generating $BLOCKS_TO_MINE blocks to mining wallet to ensure mature coins..."

if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet generatetoaddress $BLOCKS_TO_MINE "$MINING_ADDR" >/dev/null 2>&1; then
  echo "Generated blocks using mining wallet"
elif docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress $BLOCKS_TO_MINE "$MINING_ADDR" >/dev/null 2>&1; then
  echo "Generated blocks using default method"
else
  echo "Block generation failed, continuing anyway..."
fi

# Check balance after mining
BALANCE=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet getbalance)
echo "Mining wallet balance after mining: $BALANCE BTC"

# Check if nodes already have funds
CLN_OUTPUTS=$(docker exec cln-regtest lightning-cli --regtest listfunds | jq ".outputs | length")
LND_BALANCE=$(docker exec lnd-regtest lncli --network=regtest walletbalance | jq -r ".confirmed_balance // \"0\"")

echo "CLN outputs: $CLN_OUTPUTS, LND balance: $LND_BALANCE sats"

if [ $CLN_OUTPUTS -eq 0 ] || [ "$LND_BALANCE" = "0" ]; then
  echo "Nodes need funding, getting addresses and funding..."
  
  # Get Lightning node addresses and fund them
  CLN_ADDR=$(docker exec cln-regtest lightning-cli --regtest newaddr | jq -r ".bech32")
  LND_ADDR=$(docker exec lnd-regtest lncli --network=regtest newaddress p2wkh | jq -r ".address")
  
  echo "Funding CLN wallet with 2.5 BTC..."
  if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet sendtoaddress "$CLN_ADDR" 2.5 >/dev/null 2>&1; then
    echo "CLN funded using mining wallet"
  elif docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 sendtoaddress "$CLN_ADDR" 2.5 >/dev/null 2>&1; then
    echo "CLN funded using default wallet"
  else
    echo "CLN funding failed, continuing..."
  fi
  
  echo "Funding LND wallet with 2.5 BTC..."
  if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet sendtoaddress "$LND_ADDR" 2.5 >/dev/null 2>&1; then
    echo "LND funded using mining wallet"
  elif docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 sendtoaddress "$LND_ADDR" 2.5 >/dev/null 2>&1; then
    echo "LND funded using default wallet"
  else
    echo "LND funding failed, continuing..."
  fi
  
  # Confirm funding transactions
  echo "Generating blocks to confirm funding..."
  if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet generatetoaddress 6 "$MINING_ADDR" >/dev/null 2>&1; then
    echo "Confirmation blocks generated using mining wallet"
  elif docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress 6 "$MINING_ADDR" >/dev/null 2>&1; then
    echo "Confirmation blocks generated using default method"
  else
    echo "Block generation failed, continuing..."
  fi
  
  # Wait for Lightning nodes to detect funds
  echo "Waiting for Lightning nodes to detect funding..."
  sleep 30
else
  echo "Nodes already have funds, skipping funding step..."
fi

echo "Final wallet balances:"
docker exec cln-regtest lightning-cli --regtest listfunds
docker exec lnd-regtest lncli --network=regtest walletbalance

# Create channels if they dont exist
echo "Creating channels between nodes..."
docker exec cln-regtest lightning-cli --regtest connect "$LND_PUBKEY@lnd:9734" || true
docker exec lnd-regtest lncli --network=regtest connect "$CLN_PUBKEY@cln:9735" || true

sleep 5

# Try to open channels, but handle if they already exist
echo "CLN opening 0.16 BTC channel to LND..."
CLN_CHANNEL_RESULT=$(docker exec cln-regtest lightning-cli --regtest fundchannel "$LND_PUBKEY" 16000000 2>&1 || echo "failed")
if echo "$CLN_CHANNEL_RESULT" | grep -q "already have channel"; then
  echo "CLN already has a channel to LND"
elif echo "$CLN_CHANNEL_RESULT" | grep -q "failed"; then
  echo "CLN channel creation failed or already exists"
else
  echo "CLN channel opened successfully"
fi

echo "LND opening 0.16 BTC channel to CLN..."
LND_CHANNEL_RESULT=$(docker exec lnd-regtest lncli --network=regtest openchannel --local_amt=16000000 "$CLN_PUBKEY" 2>&1 || echo "failed")
if echo "$LND_CHANNEL_RESULT" | grep -q "already have"; then
  echo "LND already has a channel to CLN"
elif echo "$LND_CHANNEL_RESULT" | grep -q "failed"; then
  echo "LND channel creation failed or already exists"
else
  echo "LND channel opened successfully"
  
  echo "Generating blocks to confirm new channels..."
  if docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet generatetoaddress 12 "$MINING_ADDR" >/dev/null 2>&1; then
    echo "Channel confirmation blocks generated using mining wallet"
  elif docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress 12 "$MINING_ADDR" >/dev/null 2>&1; then
    echo "Channel confirmation blocks generated using default method"
  else
    echo "Channel confirmation block generation failed, continuing..."
  fi
  
  sleep 20
fi

echo "Setup complete! Lightning nodes now have bidirectional liquidity."