# syntax=docker/dockerfile:1.7
FROM oven/bun:1.2.17 AS server-deps
WORKDIR /app
COPY packages/common ./packages/common
COPY packages/server/package.json packages/server/bun.lock ./packages/server/
WORKDIR /app/packages/server
RUN bun install --frozen-lockfile

FROM server-deps AS server-build
WORKDIR /app/packages/server
COPY packages/server/ ./
RUN bun run typecheck \
 && bun run build

FROM rust:1.93-slim-bookworm AS rust-builder
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev libcap-dev curl ca-certificates python3 build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN curl -fsSL https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem -o /rds-global-bundle.pem

COPY Cargo.toml Cargo.lock ./
COPY vendor/codex/codex-rs ./vendor/codex/codex-rs
COPY crates/reporter-protocol/Cargo.toml   crates/reporter-protocol/Cargo.toml
COPY crates/summary-agent/Cargo.toml       crates/summary-agent/Cargo.toml
COPY crates/supermanager-cli/Cargo.toml    crates/supermanager-cli/Cargo.toml

RUN mkdir -p crates/reporter-protocol/src crates/summary-agent/src crates/supermanager-cli/src \
 && echo '// stub'       > crates/reporter-protocol/src/lib.rs \
 && echo 'fn main() {}'  > crates/summary-agent/src/main.rs \
 && echo 'fn main() {}'  > crates/supermanager-cli/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p summary-agent || true

COPY crates/reporter-protocol ./crates/reporter-protocol
COPY crates/summary-agent ./crates/summary-agent
RUN find crates/reporter-protocol crates/summary-agent -name '*.rs' -exec touch {} +
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p summary-agent \
 && cp target/release/summary-agent /summary-agent

FROM oven/bun:1.2.17-slim
WORKDIR /app/server
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=server-build /app/packages/server/.build/supermanager-server /usr/local/bin/supermanager-server
COPY --from=server-build /app/packages/server/migrations ./migrations
COPY --from=rust-builder /rds-global-bundle.pem /etc/ssl/certs/rds-global-bundle.pem
COPY --from=rust-builder /summary-agent /usr/local/bin/summary-agent
EXPOSE 8787
CMD ["/usr/local/bin/supermanager-server", "--bind", "0.0.0.0:8787"]
