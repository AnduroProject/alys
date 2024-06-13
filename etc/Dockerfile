# Start from the latest Rust base image
FROM rust:latest as builder
FROM rust:latest

SHELL ["/bin/bash", "-c"]

RUN #cargo install --git https://github.com/foundry-rs/foundry --profile local --locked forge cast chisel anvil

RUN curl -L https://foundry.paradigm.xyz | bash
RUN source ~/.bashrc
ENV PATH="~/.foundry/bin:${PATH}"
RUN foundryup

RUN apt-get update && \
    apt-get install -y cmake && \
    rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
#WORKDIR /opt
#RUN USER=root cargo new --bin alys
WORKDIR /opt/alys
#
#RUN mkdir app
#RUN mv src app
#
## Copy over your Manifest files
#COPY ./Cargo.lock ./Cargo.lock
#COPY ./Cargo.toml ./Cargo.toml
#
#COPY ./app/Cargo.toml ./app/Cargo.toml
#
#WORKDIR /opt/alys/crates
#
#RUN USER=root cargo new --lib federation
#COPY ./crates/federation/Cargo.toml ./federation/Cargo.toml
#
#RUN USER=root cargo new --bin miner
#COPY ./crates/miner/Cargo.toml ./miner/Cargo.toml
#
#WORKDIR /opt/alys
#
## This build step will cache your dependencies
#RUN cargo build --release
#RUN rm -r app crates

# Copy your source files
COPY . .

# Build for release.
# This will be much slower, but the resulting artifact will be faster.
#RUN rm ./target/release/deps/app*
RUN cargo build --release --bin app
#
## Our second stage will use Debian
#FROM debian:buster-slim
#
## Copy the build artifact from the builder stage and set the startup command
#COPY --from=builder /opt/alys/target/release/app /usr/local/bin/alys
#
## Set the startup command to run your binary
#CMD ["alys --dev"]