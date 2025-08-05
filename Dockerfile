# Build stage
FROM --platform=$BUILDPLATFORM rust:latest AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install all tools for both architectures
RUN apt-get update && apt-get install -y \
    ca-certificates \
    clang \
    cmake \
    curl \
    g++ \
    g++-aarch64-linux-gnu \
    g++-x86-64-linux-gnu \
    gcc \
    gcc-aarch64-linux-gnu \
    gcc-x86-64-linux-gnu \
    libc6-dev-amd64-cross \
    libc6-dev-arm64-cross \
    libclang-dev \
    libstdc++-12-dev \
    libstdc++-12-dev-amd64-cross \
    libstdc++-12-dev-arm64-cross \
    musl-dev \
    musl-tools \
    protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*

# Install both musl toolchains
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
         "linux/amd64") RUST_TARGET="x86_64-unknown-linux-musl" ;; \
         "linux/arm64") RUST_TARGET="aarch64-unknown-linux-musl" ;; \
    esac && \
    rustup target add ${RUST_TARGET} && \
    export PATH="/opt/x86_64-linux-musl-cross/bin:/opt/aarch64-linux-musl-cross/bin:$PATH" && \
    cargo build --release --target ${RUST_TARGET} && \
    cp /app/target/${RUST_TARGET}/release/swgr /app/swgr

#FROM gcr.io/distroless/static-debian12 AS final
FROM scratch AS final

# Copy the binary from the consistent location
COPY --from=builder /app/swgr /usr/sbin/swgr

COPY server/config/sqlite-persistent.yaml /etc/swgr/config.yaml

ENV RUST_LOG=info

CMD ["service", "--config", "/etc/swgr/config.yaml"]

ENTRYPOINT ["/usr/sbin/swgr"]