FROM lightninglabs/lnd:v0.19.3-beta

RUN apk add --no-cache openssl

COPY init-tls.sh /usr/local/bin/init-tls.sh

RUN chmod +x /usr/local/bin/init-tls.sh

ENTRYPOINT ["/usr/local/bin/init-tls.sh"]
