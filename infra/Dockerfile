FROM rust:1.100.0-bookworm AS build
RUN rustup component add rustfmt clippy
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY contracts ./contracts
COPY migrations ./migrations
COPY data/stations ./data/stations
RUN cargo build --release --locked -p emberwatch-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 emberwatch
WORKDIR /app
COPY --from=build /app/target/release/emberwatch-server /usr/local/bin/emberwatch-server
RUN mkdir -p /app/.state && chown -R emberwatch:emberwatch /app
USER emberwatch
ENV BIND_ADDRESS=0.0.0.0:8080
EXPOSE 8080
CMD ["emberwatch-server"]
