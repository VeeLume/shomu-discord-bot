# syntax=docker/dockerfile:1.7

############################
# Builder (musl)
############################
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev pkgconfig build-base ca-certificates
ENV SQLX_OFFLINE=true
WORKDIR /app

# Cache deps
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && printf "fn main() {}\n" > src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl || true

# Real build
RUN rm -rf src
COPY . .
RUN rustup target add x86_64-unknown-linux-musl
RUN cargo build --release --target x86_64-unknown-linux-musl

# Prepare an owned data dir we can copy into scratch
RUN mkdir -p /data-owned

############################
# Runtime (scratch)
############################
FROM scratch

# TLS certs for HTTPS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# App binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/shomu-discord-bot /app

# Pre-create /data owned by non-root
COPY --from=builder --chown=10001:10001 /data-owned /data

# Run as non-root
USER 10001:10001

# Defaults (UTC)
ENV RUST_LOG=info \
    DATABASE_URL=sqlite:///data/bot.db \
    TZ=UTC

VOLUME ["/data"]

# Use --init in run/compose for clean PID1 handling
ENTRYPOINT ["/app"]
