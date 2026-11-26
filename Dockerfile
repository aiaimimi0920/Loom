FROM rust:1.99.1-slim-bookworm AS builder

ARG LOOM_EDITION=web

RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential ca-certificates libssl-dev pkg-config \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY protocol ./protocol

RUN --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git \
  if [ "$LOOM_EDITION" = "api" ]; then \
    cargo build --locked --release --no-default-features --features runtime-defaults -p loom-daemon && \
    cargo build --locked --release -p loom-cli; \
  else \
    cargo build --locked --release --no-default-features --features "runtime-defaults,management-ui" -p loom-daemon && \
    cargo build --locked --release -p loom-cli; \
  fi

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 perl-base bubblewrap \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/loom-daemon /usr/local/bin/loom-daemon
COPY --from=builder /app/target/release/loom /usr/local/bin/loom

ENV LOOM_DAEMON_HOST=0.0.0.0
ENV LOOM_DAEMON_PORT=8765
ENV LOOM_ALLOW_LAN_MANAGEMENT=1

EXPOSE 8765

CMD ["loom-daemon"]
