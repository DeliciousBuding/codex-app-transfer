# Builder stage
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

RUN cargo build --release --features server --bin codex-app-transfer-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/codex-app-transfer-server /usr/local/bin/

RUN mkdir -p /root/.codex-app-transfer

VOLUME ["/root/.codex-app-transfer"]

EXPOSE 18081
EXPOSE 18080

ENV PORT=18081

ENTRYPOINT ["codex-app-transfer-server"]
