FROM docker:24-cli

RUN apk add --no-cache jq

COPY init.sh /init.sh
COPY ln.sh /ln.sh
COPY credentials.sh /credentials.sh
RUN chmod +x /init.sh /ln.sh /credentials.sh

CMD ["/init.sh"]