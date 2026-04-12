# syntax=docker/dockerfile:1.7
FROM rust:1.93-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev libcap-dev curl ca-certificates python3 build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# ---- Layer 1: dependency manifests + vendored codex (rarely changes) ----
# vendor/codex is a path dep so its full source must be present, but it only
# changes when the submodule is bumped.
COPY Cargo.toml Cargo.lock ./
COPY vendor/codex ./vendor/codex
COPY crates/coordination-server/Cargo.toml crates/coordination-server/Cargo.toml
COPY crates/reporter-protocol/Cargo.toml   crates/reporter-protocol/Cargo.toml
COPY crates/supermanager-cli/Cargo.toml    crates/supermanager-cli/Cargo.toml

# Stub each workspace member so cargo can compile the dep graph without real
# sources. coordination-server is a bin, reporter-protocol is a lib,
# supermanager-cli has both.
RUN mkdir -p crates/coordination-server/src \
             crates/reporter-protocol/src \
             crates/supermanager-cli/src \
 && echo 'fn main() {}'  > crates/coordination-server/src/main.rs \
 && echo '// stub'       > crates/reporter-protocol/src/lib.rs \
 && echo '// stub'       > crates/supermanager-cli/src/lib.rs \
 && echo 'fn main() {}'  > crates/supermanager-cli/src/main.rs

# Warm the dep cache. Cache mounts persist across builds on the same builder.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p coordination-server || true

# ---- Layer 2: real sources (changes on every code edit) ----
COPY crates ./crates
# Touch sources so cargo notices the stubs were replaced.
RUN find crates -name '*.rs' -exec touch {} +
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p coordination-server \
 && cp target/release/coordination-server /coordination-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /coordination-server /usr/local/bin/coordination-server
EXPOSE 8787
CMD ["coordination-server", "--bind", "0.0.0.0:8787"]
