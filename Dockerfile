# =============================================================================
# Homun — Multi-stage Docker build
# Usage:
#   docker compose up              (recommended — includes Caddy reverse proxy)
#   docker build -t homun .        (standalone build)
# =============================================================================

# === Stage 1: Build ===
FROM rust:1.85-bookworm AS builder

WORKDIR /build

# Cache dependency build (layer reused unless Cargo.toml/Cargo.lock change)
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() { println!("placeholder"); }' > src/main.rs \
    && cargo build --release --features full 2>/dev/null ; true
RUN rm -rf src

# Copy real source and build
COPY src/ src/
COPY migrations/ migrations/
COPY static/ static/
COPY skills/ skills/
RUN touch src/main.rs \
    && cargo build --release --features full \
    && strip target/release/homun

# === Stage 2: Runtime ===
# node:22-bookworm-slim provides Node.js for browser automation (npx @playwright/mcp)
FROM node:22-bookworm-slim

LABEL org.opencontainers.image.title="Homun" \
      org.opencontainers.image.description="Personal AI assistant — a digital homunculus" \
      org.opencontainers.image.source="https://github.com/homunbot/homun" \
      org.opencontainers.image.licenses="PolyForm-Noncommercial-1.0.0"

# Runtime dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates \
       tini \
       curl \
       git \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash homun

# Copy binary from builder
COPY --from=builder /build/target/release/homun /usr/local/bin/homun

# Create data directory structure
RUN mkdir -p /home/homun/.homun/memory \
             /home/homun/.homun/skills \
             /home/homun/.homun/brain \
             /home/homun/.homun/rag \
             /home/homun/.homun/workspace \
    && chown -R homun:homun /home/homun

# Seed Docker-friendly config (overridden by user config in volume)
COPY docker/config.docker.toml /home/homun/.homun/config.toml
RUN chown homun:homun /home/homun/.homun/config.toml

USER homun
WORKDIR /home/homun

ENV HOME=/home/homun
ENV RUST_LOG=info

EXPOSE 18080

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -sf http://localhost:18080/api/health || exit 1

ENTRYPOINT ["tini", "--"]
CMD ["homun", "gateway"]
