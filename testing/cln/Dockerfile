FROM elementsproject/lightningd:v25.09

RUN apt-get update && apt-get install -y openssl && rm -rf /var/lib/apt/lists/*

COPY init-tls.sh /usr/local/bin/init-tls.sh

RUN chmod +x /usr/local/bin/init-tls.sh

ENTRYPOINT ["/usr/local/bin/init-tls.sh"]
