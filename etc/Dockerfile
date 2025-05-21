# Start from the latest Rust base image
FROM rust:bullseye AS planner
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
#RUN cargo install cargo-chef

#FROM chef AS planner
#WORKDIR /opt/alys
#
#COPY . .
#RUN cargo chef prepare --recipe-path recipe.json

#------------------------------------------------------------------------------------------
FROM planner AS system-pkg-cache

RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake ninja-build && \
        apt-get clean && \
        rm -rf /var/lib/apt/lists/*
#------------------------------------------------------------------------------------------

FROM system-pkg-cache AS builder
WORKDIR /opt/alys

# Create mount points for cargo cache
#RUN mkdir -p /usr/local/cargo/registry /usr/local/cargo/git

#COPY --from=planner /opt/alys/recipe.json recipe.json

#RUN #--mount=type=cache,target=/usr/local/cargo/registry \
#  --mount=type=cache,target=/usr/local/cargo/git \
#  cargo chef cook --recipe-path recipe.json

COPY . .

##RUN --mount=type=cache,target=/usr/local/cargo/registry \
#  --mount=type=cache,target=/usr/local/cargo/git \
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/home/root/app/target \
    --mount=type=cache,target=/usr/local/cargo/git \
     cargo build --lib --verbose
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/home/root/app/target \
    --mount=type=cache,target=/usr/local/cargo/git \
      cargo build --bin app --verbose

#------------------------------------------------------------------------------------------

## Our second stage will use Debian
FROM debian:bullseye-slim AS runtime
#
## Copy the build artifact from the builder stage and set the startup command
#COPY --from=builder /opt/alys/target/release/app /bin/alys
COPY --from=builder /opt/alys/target/debug/app /bin/alys
#
## Set the startup command to run your binary
CMD ["alys --dev"]
