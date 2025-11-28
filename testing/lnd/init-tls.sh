#!/bin/sh
set -e

CERT_DIR="/root/.lnd"
mkdir -p "$CERT_DIR"

HOSTNAME="${LND_HOSTNAME:-lnd}"

cat > "$CERT_DIR/openssl.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = $HOSTNAME

[v3_req]
subjectAltName = @alt_names

[alt_names]
DNS.1 = $HOSTNAME
DNS.2 = localhost
EOF

openssl req -new -x509 -days 365 -nodes \
    -out "$CERT_DIR/tls.cert" \
    -keyout "$CERT_DIR/tls.key" \
    -outform PEM \
    -config "$CERT_DIR/openssl.cnf"

rm "$CERT_DIR/openssl.cnf"

chmod 600 "$CERT_DIR/tls.key"
chmod 644 "$CERT_DIR/tls.cert"

exec lnd \
    --bitcoin.active \
    --bitcoin.regtest \
    --bitcoin.node=bitcoind \
    --bitcoind.rpchost=bitcoin:18443 \
    --bitcoind.rpcuser=bitcoin \
    --bitcoind.rpcpass=bitcoin123 \
    --bitcoind.zmqpubrawblock=tcp://bitcoin:28332 \
    --bitcoind.zmqpubrawtx=tcp://bitcoin:28333 \
    --rpclisten=0.0.0.0:${LND_PORT} \
    --restlisten=0.0.0.0:8080 \
    --listen=0.0.0.0:9734 \
    --externalip=lnd:9734 \
    --tlscertpath="$CERT_DIR/tls.cert" \
    --tlskeypath="$CERT_DIR/tls.key" \
    --noseedbackup \
    --accept-keysend \
    --accept-amp \
    --debuglevel=info
