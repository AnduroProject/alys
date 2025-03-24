# Start from the latest Rust base image
FROM rust:bullseye AS chef
RUN cargo install cargo-chef

FROM chef AS planner
WORKDIR /opt/alys

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

#------------------------------------------------------------------------------------------
FROM planner AS system-pkg-cache

RUN apt-get update && \
    apt-get install -y cmake && \
    rm -rf /var/lib/apt/lists/*

#------------------------------------------------------------------------------------------

FROM system-pkg-cache AS builder
WORKDIR /opt/alys
COPY --from=planner /opt/alys/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release --bin app

#------------------------------------------------------------------------------------------

## Our second stage will use Debian
FROM debian:bullseye-slim AS runtime
#
## Copy the build artifact from the builder stage and set the startup command
COPY --from=builder /opt/alys/target/release/app /bin/alys
#
## Set the startup command to run your binary
CMD ["alys --dev"]
