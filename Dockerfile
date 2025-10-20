# syntax=docker/dockerfile:1.7

############################
# Builder image
############################
FROM rust:1-bookworm AS builder

# If you use sqlx::query! macros, you want offline mode so the build
# doesn't need a live DB connection. This expects sqlx-data.json in repo.
ENV SQLX_OFFLINE=true

# Build deps for sqlite
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libsqlite3-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# --- Dependency caching trick ---
# 1) copy manifests only
COPY Cargo.toml Cargo.lock ./

# 2) fake src to prebuild deps
RUN mkdir -p src && printf "fn main() {}\n" > src/main.rs

# 3) prebuild to cache dependencies
RUN cargo build --release || true

# 4) now bring in real source
RUN rm -rf src
COPY . .

# Build the real binary
RUN cargo build --release

############################
# Runtime image
############################
FROM debian:bookworm-slim AS runtime

# Only the bare runtime deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libsqlite3-0 tzdata dumb-init \
 && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -u 10001 appuser

# App directory for data (SQLite file lives here by default)
RUN mkdir -p /data && chown -R appuser:appuser /data

# Copy the binary
COPY --from=builder /app/target/release/shomu-discord-bot /usr/local/bin/app

ENV RUST_LOG=info \
    # Default DB path; override in docker run/compose if you prefer
    DATABASE_URL=sqlite:///data/bot.db

VOLUME ["/data"]

USER appuser

ENTRYPOINT ["dumb-init","--"]
CMD ["/usr/local/bin/app"]
