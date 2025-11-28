#!/bin/sh
set -e

CERT_DIR="/etc/mysql/certs"
mkdir -p "$CERT_DIR"

HOSTNAME="${MYSQL_HOSTNAME:-mysql}"

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

chown mysql:mysql "$CERT_DIR/server.pem" "$CERT_DIR/server.key"
chmod 644 "$CERT_DIR/server.pem"
chmod 600 "$CERT_DIR/server.key"

exec docker-entrypoint.sh mysqld \
    --ssl-ca="$CERT_DIR/server.pem" \
    --ssl-cert="$CERT_DIR/server.pem" \
    --ssl-key="$CERT_DIR/server.key" \
    --require_secure_transport=OFF \
    --log-error-verbosity="${MYSQL_LOG_ERROR_VERBOSITY:-2}" \
    --bind-address=0.0.0.0
