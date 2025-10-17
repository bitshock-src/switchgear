FROM rust:1.90.0

RUN apt-get update && \
    apt-get upgrade -y && \
    apt-get install -y \
        clang \
        cmake \
        libclang-dev \
        protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*
