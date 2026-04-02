FROM rust:1.89-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p coordination-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/coordination-server /usr/local/bin/
EXPOSE 8787
CMD ["coordination-server", "--bind", "0.0.0.0:8787", "--db-path", "/data/supermanager.db"]
