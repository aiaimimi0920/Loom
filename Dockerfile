FROM rust:1.95.0-slim-bookworm AS builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential ca-certificates libssl-dev pkg-config \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY examples ./examples
COPY protocol ./protocol

RUN --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git \
  cargo build --locked --release -p loom-daemon -p loom-cli

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/loom-daemon /usr/local/bin/loom-daemon
COPY --from=builder /app/target/release/loom /usr/local/bin/loom

ENV LOOM_DAEMON_HOST=0.0.0.0
ENV LOOM_DAEMON_PORT=8765

EXPOSE 8765

CMD ["loom-daemon"]
