#!/bin/sh
# Fetch the TLS cert + key and the bearer token for the OTLP gRPC receiver
# from the credentials-server (material is shipped over HTTP, never mounted),
# then run the collector.
set -e

CS="http://${CREDENTIALS_SERVER_HOSTNAME}:${CREDENTIALS_SERVER_PORT}"
wget -qO /tmp/otel_collector_cert "${CS}/credentials/otel-collector/cert.pem"
wget -qO /tmp/otel_collector_key  "${CS}/credentials/otel-collector/key.pem"
wget -qO /tmp/otel_collector_client_ca "${CS}/credentials/otel-collector/ca.pem"
wget -qO /tmp/otel_collector_token "${CS}/credentials/otel-collector/token"

exec /otelcol-contrib --config /etc/otelcol-contrib/config.yaml
