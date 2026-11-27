FROM rust:1.100-bookworm AS builder

WORKDIR /workspace
COPY . .
RUN cargo build --release --locked --bin backend

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /workspace/target/release/backend /usr/local/bin/backend
COPY --from=builder /workspace/migrations ./migrations
COPY --from=builder /workspace/config.docker.toml /app/config.toml
RUN mkdir -p /app/logs \
    && chown -R 10001:10001 /app

USER 10001:10001
EXPOSE 6080

ENTRYPOINT ["/usr/local/bin/backend"]
