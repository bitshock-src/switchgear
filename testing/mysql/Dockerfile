FROM mysql:8.0

COPY init-tls.sh /usr/local/bin/init-tls.sh

RUN chmod +x /usr/local/bin/init-tls.sh

ENTRYPOINT ["/usr/local/bin/init-tls.sh"]
