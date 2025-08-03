FROM docker:24-cli

# Install jq for JSON parsing
RUN apk add --no-cache jq

# Copy the setup scripts
COPY init.sh /init.sh
COPY bitcoin.sh /bitcoin.sh
COPY ln.sh /ln.sh
COPY credentials.sh /credentials.sh
RUN chmod +x /init.sh /bitcoin.sh /ln.sh /credentials.sh

# Run the initialization orchestrator
CMD ["/init.sh"]