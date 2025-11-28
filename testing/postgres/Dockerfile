FROM postgres:16-alpine

RUN apk add --no-cache openssl

COPY init-tls.sh /usr/local/bin/init-tls.sh

RUN chmod +x /usr/local/bin/init-tls.sh

ENTRYPOINT ["/usr/local/bin/init-tls.sh"]
