## syntax=docker/dockerfile:1.7

# ---- Build stage ----
FROM rust:1.88-slim AS builder
WORKDIR /app

# Cache dependencies separately from source for fast rebuilds
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src target/release/deps/clear_urls_bot* \
           target/release/clear_urls_bot* 2>/dev/null || true

# Real source build
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---- Runtime stage (distroless, ~25 MB total image) ----
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder /app/target/release/clear_urls_bot /app/bot
USER nonroot
ENTRYPOINT ["/app/bot"]
