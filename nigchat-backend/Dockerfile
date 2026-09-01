# ---- build ----------------------------------------------------------------
# 1.85+ is required: the dependency tree contains crates using edition 2024.
# `1-slim` tracks the latest stable 1.x, which is what rust-toolchain.toml asks for.
FROM rust:1-slim AS builder

WORKDIR /build
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Dependencies are built from the manifests alone and cached as their own
# layer, so editing source does not rebuild the whole dependency tree.
COPY Cargo.toml Cargo.lock* ./
COPY crates/domain/Cargo.toml         crates/domain/Cargo.toml
COPY crates/application/Cargo.toml    crates/application/Cargo.toml
COPY crates/infrastructure/Cargo.toml crates/infrastructure/Cargo.toml
COPY crates/api/Cargo.toml            crates/api/Cargo.toml
COPY crates/server/Cargo.toml         crates/server/Cargo.toml

RUN mkdir -p crates/domain/src crates/application/src crates/infrastructure/src \
             crates/api/src crates/server/src \
 && echo "" > crates/domain/src/lib.rs \
 && echo "" > crates/application/src/lib.rs \
 && echo "" > crates/infrastructure/src/lib.rs \
 && echo "" > crates/api/src/lib.rs \
 && echo "fn main() {}" > crates/server/src/main.rs \
 && cargo build --release \
 && rm -rf crates/*/src

COPY crates ./crates
COPY migrations ./migrations

# Touch so cargo notices the real sources replaced the stubs.
RUN find crates -name "*.rs" -exec touch {} + \
 && cargo build --release --bin nigchat-server

# ---- runtime --------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# Never run as root.
RUN useradd --create-home --uid 10001 nigchat
WORKDIR /app

COPY --from=builder /build/target/release/nigchat-server /usr/local/bin/nigchat-server
COPY --chown=nigchat:nigchat migrations ./migrations

USER nigchat
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
  CMD curl -fsS http://localhost:8080/healthz || exit 1

CMD ["nigchat-server"]
