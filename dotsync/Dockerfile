FROM rust:latest AS builder

WORKDIR /build

RUN rustup target add aarch64-unknown-linux-musl && \
    apt-get update && apt-get install -y \
    musl-tools \
    git \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --target aarch64-unknown-linux-musl

FROM almalinux:9-minimal

RUN microdnf update -y && \
    microdnf install -y \
    git \
    vim \
    ca-certificates && \
    microdnf clean all

RUN useradd -m -s /bin/bash appuser

WORKDIR /home/appuser

COPY --from=builder /build/target/aarch64-unknown-linux-musl/release/dotsync /usr/local/bin/dotsync

COPY .dotsync.json ./.dotsync.json

RUN chown -R appuser:appuser /home/appuser && \
    chmod 755 /usr/local/bin/dotsync

USER appuser
