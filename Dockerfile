## syntax=docker/dockerfile:1.7

# ---- Build stage ----
FROM rust:1.88-slim AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Cache dependencies separately from source for fast rebuilds
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src target/release/deps/url_cleanse_bot* \
           target/release/url_cleanse_bot* 2>/dev/null || true

# Real source build
COPY src ./src
RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    touch src/main.rs && cargo build --release --locked

# ---- Runtime stage (distroless, ~25 MB total image) ----
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app

# OCI image labels
LABEL org.opencontainers.image.source="https://github.com/good-wine/urlcleansebot"
LABEL org.opencontainers.image.description="Telegram bot that removes tracking parameters from shared URLs"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.title="URLCleanseBot"

COPY --from=builder /app/target/release/url_cleanse_bot /app/bot
USER nonroot
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["/app/bot", "--health"]
ENTRYPOINT ["/app/bot"]
