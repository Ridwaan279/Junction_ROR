FROM rust:1.100.0-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY contracts ./contracts
COPY migrations ./migrations
# Compiled in: drone station catalogue (crates/server/src/fleet_api.rs).
COPY data/stations ./data/stations
RUN cargo build --release --locked -p emberwatch-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 emberwatch
WORKDIR /app
COPY --from=build /app/target/release/emberwatch-server /usr/local/bin/emberwatch-server
# Agent 2's generated replay, served by /api/incidents/arganil-early/fleet and /exposures.
COPY web/public/generated/arganil-early /app/generated/arganil-early
RUN mkdir -p /app/.state && chown -R emberwatch:emberwatch /app
USER emberwatch
ENV BIND_ADDRESS=0.0.0.0:8080 \
    EMBERWATCH_GENERATED_DIR=/app/generated/arganil-early \
    STATE_PATH=/app/.state/platform.json
# Live Fogos.pt checks stay off until the host sets CONTEXT_DEV_API_KEY (secret),
# EMBERWATCH_LIVE_ENABLED=1, FOGOS_PUBLIC_PAGE_CHECKS_PASSED=1 and WEB_ORIGIN.
EXPOSE 8080
CMD ["emberwatch-server"]
