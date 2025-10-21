#!/bin/sh
set -e

CLN_INFO=$(docker exec cln-regtest lightning-cli --regtest getinfo)
LND_INFO=$(docker exec lnd-regtest lncli --network=regtest getinfo)

CLN_PUBKEY=$(echo "$CLN_INFO" | jq -r ".id")
LND_PUBKEY=$(echo "$LND_INFO" | jq -r ".identity_pubkey")

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 createwallet mining_wallet false false "" false true false

MINING_ADDR=$(docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet getnewaddress)

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress 200 "$MINING_ADDR"

CLN_ADDR=$(docker exec cln-regtest lightning-cli --regtest newaddr | jq -r ".bech32")
LND_ADDR=$(docker exec lnd-regtest lncli --network=regtest newaddress p2wkh | jq -r ".address")

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet sendtoaddress "$CLN_ADDR" 2.5

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 -rpcwallet=mining_wallet sendtoaddress "$LND_ADDR" 2.5

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress 6 "$MINING_ADDR"

while [ "$(docker exec cln-regtest lightning-cli --regtest listfunds | jq '.outputs | length')" = "0" ]; do
  sleep 1
done

while [ "$(docker exec lnd-regtest lncli --network=regtest walletbalance | jq -r '.confirmed_balance')" = "0" ]; do
  sleep 1
done

docker exec cln-regtest lightning-cli --regtest listfunds
docker exec lnd-regtest lncli --network=regtest walletbalance

docker exec cln-regtest lightning-cli --regtest connect "$LND_PUBKEY@lnd:9734" || true
docker exec lnd-regtest lncli --network=regtest connect "$CLN_PUBKEY@cln:9735" || true

while [ "$(docker exec cln-regtest lightning-cli --regtest listpeers | jq '.peers | length')" = "0" ]; do
  docker exec cln-regtest lightning-cli --regtest connect "$LND_PUBKEY@lnd:9734" 2>/dev/null || true
  sleep 1
done

while [ "$(docker exec lnd-regtest lncli --network=regtest listpeers | jq '.peers | length')" = "0" ]; do
  docker exec lnd-regtest lncli --network=regtest connect "$CLN_PUBKEY@cln:9735" 2>/dev/null || true
  sleep 1
done

docker exec cln-regtest lightning-cli --regtest fundchannel "$LND_PUBKEY" 16000000

docker exec lnd-regtest lncli --network=regtest openchannel --local_amt=16000000 "$CLN_PUBKEY"

docker exec bitcoin-regtest bitcoin-cli -regtest -rpcuser=bitcoin -rpcpassword=bitcoin123 generatetoaddress 12 "$MINING_ADDR"

while [ "$(docker exec cln-regtest lightning-cli --regtest listchannels | jq '.channels | length')" -lt "2" ]; do
  sleep 1
done

while [ "$(docker exec lnd-regtest lncli --network=regtest listchannels | jq '.channels | length')" -lt "2" ]; do
  sleep 1
done

echo "LN setup complete"