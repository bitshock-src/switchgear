ARG BUILDPLATFORM
ARG WEBPKI_ROOTS=false

FROM --platform=$BUILDPLATFORM bitshock/linux-musl-rust:1.91.1 AS builder

WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock ./

COPY server/src ./server/src
COPY server/Cargo.toml ./server/Cargo.toml

COPY service/src ./service/src
COPY service/proto ./service/proto
COPY service/build.rs ./service/build.rs
COPY service/Cargo.toml ./service/Cargo.toml

COPY pingora/src ./pingora/src
COPY pingora/Cargo.toml ./pingora/Cargo.toml

COPY migration/src ./migration/src
COPY migration/Cargo.toml ./migration/Cargo.toml

COPY switchgear/src ./switchgear/src
COPY switchgear/Cargo.toml ./switchgear/Cargo.toml

COPY testing/Cargo.toml ./testing/Cargo-src.toml

RUN sed '/^\[dependencies\]/q' ./testing/Cargo-src.toml > ./testing/Cargo.toml

RUN mkdir -p ./testing/src && touch ./testing/src/lib.rs

ARG TARGETPLATFORM

RUN case ${TARGETPLATFORM} in \
         "linux/amd64") echo "RUST_TARGET=x86_64-unknown-linux-musl" > /app/build.env ;; \
         "linux/arm64") echo "RUST_TARGET=aarch64-unknown-linux-musl" > /app/build.env ;; \
         *) echo "Unsupported platform: ${TARGETPLATFORM}" && exit 1 ;; \
    esac

RUN . /app/build.env && \
    cargo build --release \
    --config /opt/rust/linux-musl-rust.toml \
    --target ${RUST_TARGET} && \
    cp /app/target/${RUST_TARGET}/release/swgr /app/swgr

FROM scratch AS webpki-roots-true
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

FROM scratch AS webpki-roots-false

FROM webpki-roots-$WEBPKI_ROOTS AS final
COPY --from=builder /app/swgr /usr/sbin/swgr
COPY server/config/persistence.yaml /etc/swgr/config.yaml
ENV RUST_LOG=info
CMD ["service", "--config", "/etc/swgr/config.yaml"]
ENTRYPOINT ["/usr/sbin/swgr"]

