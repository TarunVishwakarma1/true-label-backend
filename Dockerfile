# syntax=docker/dockerfile:1

# cargo-chef splits "resolve deps" from "compile my code" into separate
# layers keyed on Cargo.lock, so editing src/ doesn't invalidate the (slow)
# dependency-compile layer — only touching Cargo.toml/Cargo.lock does.
FROM lukemathwalker/cargo-chef:latest-rust-1-slim-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin truelabel-backend

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin appuser

WORKDIR /app
COPY --from=builder /app/target/release/truelabel-backend /app/truelabel-backend

USER appuser
EXPOSE 8080
ENTRYPOINT ["/app/truelabel-backend"]
