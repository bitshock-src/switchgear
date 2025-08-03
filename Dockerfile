# Build stage
FROM rust:latest AS builder

ARG TARGETPLATFORM

RUN apt-get update && apt-get install -y \
    musl-tools \
    musl-dev \
    ca-certificates \
    gcc \
    cmake \
    curl \
    protobuf-compiler \
    clang \
    libclang-dev \
    g++ \
    g++-aarch64-linux-gnu \
    libstdc++-12-dev \
    libstdc++-12-dev-arm64-cross && rm -rf /var/lib/apt/lists/*

RUN curl -L https://musl.cc/aarch64-linux-musl-cross.tgz | tar xzf - -C /opt && \
    curl -L https://musl.cc/x86_64-linux-musl-cross.tgz | tar xzf - -C /opt

WORKDIR /app
COPY Cargo.toml ./
COPY server/build-image-cargo-config.toml .cargo/config.toml

COPY server/src ./server/src
COPY server/Cargo.toml ./server/Cargo.toml

COPY service/src ./service/src
COPY service/Cargo.toml ./service/Cargo.toml

COPY pingora/src ./pingora/src
COPY pingora/Cargo.toml ./pingora/Cargo.toml

COPY migration/src ./migration/src
COPY migration/Cargo.toml ./migration/Cargo.toml

COPY testing/Cargo-empty.toml ./testing/Cargo.toml

RUN mkdir -p ./testing/src && touch ./testing/src/lib.rs

RUN case ${TARGETPLATFORM} in \
         "linux/amd64") RUST_TARGET="x86_64-unknown-linux-musl"; CROSS_PREFIX="x86_64" ;; \
         "linux/arm64") RUST_TARGET="aarch64-unknown-linux-musl"; CROSS_PREFIX="aarch64" ;; \
    esac && \
    rustup target add ${RUST_TARGET} && \
    export PATH="/opt/${CROSS_PREFIX}-linux-musl-cross/bin:$PATH" && \
    cargo build --release --target ${RUST_TARGET}

#FROM gcr.io/distroless/static-debian12 AS final
FROM scratch AS final

ARG TARGETPLATFORM
ARG TARGETARCH

FROM final AS final-amd64
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/swgr /usr/sbin/swgr

FROM final AS final-arm64  
COPY --from=builder /app/target/aarch64-unknown-linux-musl/release/swgr /usr/sbin/swgr

FROM final-${TARGETARCH}

COPY server/config/sqlite-persistent.yaml /etc/swgr/config.yaml

ENV RUST_LOG=info

CMD ["service", "--config", "/etc/swgr/config.yaml"]

ENTRYPOINT ["/usr/sbin/swgr"]