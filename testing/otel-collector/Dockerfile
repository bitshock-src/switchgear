# The upstream collector image is distroless (no shell), but the TLS receiver
# needs a cert + key + bearer-token on disk that we fetch from the
# credentials-server at startup. Repackage the static collector binary on
# alpine so an entrypoint script can wget the material before exec'ing the
# collector.
FROM otel/opentelemetry-collector-contrib:0.145.0 AS upstream

FROM alpine:3.20
RUN apk add --no-cache wget
COPY --from=upstream /otelcol-contrib /otelcol-contrib
COPY otel-collector-config.yml /etc/otelcol-contrib/config.yaml
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
