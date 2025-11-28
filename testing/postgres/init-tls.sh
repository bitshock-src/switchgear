#!/bin/sh
set -e

CERT_DIR="/var/lib/postgresql"

HOSTNAME="${POSTGRES_HOSTNAME:-postgres}"

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
    -out "$CERT_DIR/server.pem" \
    -keyout "$CERT_DIR/server.key" \
    -outform PEM \
    -config "$CERT_DIR/openssl.cnf"

rm "$CERT_DIR/openssl.cnf"

chown postgres:postgres "$CERT_DIR/server.pem" "$CERT_DIR/server.key"
chmod 600 "$CERT_DIR/server.key"
chmod 644 "$CERT_DIR/server.pem"

exec docker-entrypoint.sh postgres \
    -c ssl=on \
    -c ssl_cert_file="$CERT_DIR/server.pem" \
    -c ssl_key_file="$CERT_DIR/server.key" \
    -c log_min_messages="${POSTGRES_LOG_MIN_MESSAGES:-info}" \
    -c listen_addresses='*'
