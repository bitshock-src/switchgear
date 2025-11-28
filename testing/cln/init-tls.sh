#!/bin/sh
set -e

CERT_DIR="/root/.lightning/regtest"
mkdir -p "$CERT_DIR"

HOSTNAME="${CLN_HOSTNAME:-cln}"

# Generate CA certificate and key
cat > "$CERT_DIR/openssl-ca.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no

[req_distinguished_name]
CN = $HOSTNAME CA

[v3_ca]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer
basicConstraints = critical, CA:true
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
EOF


openssl req -new -x509 -days 365 -nodes \
    -out "$CERT_DIR/ca.pem" \
    -keyout "$CERT_DIR/ca-key.pem" \
    -outform PEM \
    -config "$CERT_DIR/openssl-ca.cnf"

# Generate server certificate and key
cat > "$CERT_DIR/openssl-server.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = $HOSTNAME

[v3_req]
subjectAltName = @alt_names
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth

[alt_names]
DNS.1 = $HOSTNAME
DNS.2 = localhost
EOF

# Generate server private key
openssl genrsa -out "$CERT_DIR/server-key.pem" 2048

# Create server certificate signing request
openssl req -new \
    -key "$CERT_DIR/server-key.pem" \
    -out "$CERT_DIR/server.csr" \
    -config "$CERT_DIR/openssl-server.cnf"

# Sign server certificate with CA
openssl x509 -req \
    -in "$CERT_DIR/server.csr" \
    -CA "$CERT_DIR/ca.pem" \
    -CAkey "$CERT_DIR/ca-key.pem" \
    -CAcreateserial \
    -out "$CERT_DIR/server.pem" \
    -days 365 \
    -extensions v3_req \
    -extfile "$CERT_DIR/openssl-server.cnf"

# Generate client certificate and key
cat > "$CERT_DIR/openssl-client.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = $HOSTNAME client

[v3_req]
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF

# Generate client private key
openssl genrsa -out "$CERT_DIR/client-key.pem" 2048

# Create client certificate signing request
openssl req -new \
    -key "$CERT_DIR/client-key.pem" \
    -out "$CERT_DIR/client.csr" \
    -config "$CERT_DIR/openssl-client.cnf"

# Sign client certificate with CA
openssl x509 -req \
    -in "$CERT_DIR/client.csr" \
    -CA "$CERT_DIR/ca.pem" \
    -CAkey "$CERT_DIR/ca-key.pem" \
    -CAcreateserial \
    -out "$CERT_DIR/client.pem" \
    -days 365 \
    -extensions v3_req \
    -extfile "$CERT_DIR/openssl-client.cnf"

# Clean up temporary files
rm "$CERT_DIR/openssl-ca.cnf" \
   "$CERT_DIR/openssl-server.cnf" \
   "$CERT_DIR/openssl-client.cnf" \
   "$CERT_DIR/server.csr" \
   "$CERT_DIR/client.csr"

# Set proper permissions
chmod 600 "$CERT_DIR/ca-key.pem" "$CERT_DIR/server-key.pem" "$CERT_DIR/client-key.pem"
chmod 644 "$CERT_DIR/ca.pem" "$CERT_DIR/server.pem" "$CERT_DIR/client.pem"

exec lightningd \
    --network=regtest \
    --bitcoin-rpcconnect=bitcoin \
    --bitcoin-rpcport=18443 \
    --bitcoin-rpcuser=bitcoin \
    --bitcoin-rpcpassword=bitcoin123 \
    --grpc-port=${CLN_PORT} \
    --grpc-host=0.0.0.0 \
    --log-level=info \
    --bind-addr=0.0.0.0:9735 \
    --announce-addr=cln:9735
