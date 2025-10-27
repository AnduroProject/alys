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
    apt-get install -y --no-install-recommends \
        cmake \
        ninja-build \
        clang \
        libclang-dev \
        llvm-dev \
        git \
        ca-certificates && \
    update-ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
#------------------------------------------------------------------------------------------

FROM system-pkg-cache AS builder
WORKDIR /opt/alys

# Install rustfmt component (required for some build scripts)
RUN rustup component add rustfmt

# Create mount points for cargo cache
#RUN mkdir -p /usr/local/cargo/registry /usr/local/cargo/git

#COPY --from=planner /opt/alys/recipe.json recipe.json

#RUN #--mount=type=cache,target=/usr/local/cargo/registry \
#  --mount=type=cache,target=/usr/local/cargo/git \
#  cargo chef cook --recipe-path recipe.json

COPY . .

# --- Step 3: cleanup stale/partial cargo git clones and run a quick git reachability check ---
# This removes only lighthouse-related partial clones and runs `git ls-remote` against the exact SHA.
RUN set -eux; \
    # ensure git available (installed in system-pkg-cache stage above)
    git --version || (apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*); \
    echo "Checking remote for lighthouse commit 441fc1691b69f9edc4bbdc6665f3efab16265c9b"; \
    git ls-remote https://github.com/sigp/lighthouse 441fc1691b69f9edc4bbdc6665f3efab16265c9b || true; \
    # targeted cleanup of possible corrupt/stale cargo git DB entries (safe)
    rm -rf /usr/local/cargo/git/db/lighthouse-* /usr/local/cargo/git/checkouts/lighthouse-* || true

# 2) Force cargo to use the system Git CLI (avoids libgit2 issues under QEMU)
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Optional: Work around HTTP/2/TLS oddities sometimes seen via QEMU
RUN git config --global http.version HTTP/1.1

# 3) Fetch first (fail fast on git issues, with cache mounts), then build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch --locked || cargo fetch

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --bin app --verbose

##RUN --mount=type=cache,target=/usr/local/cargo/registry \
#  --mount=type=cache,target=/usr/local/cargo/git \
#RUN --mount=type=cache,target=/usr/local/cargo/registry \
#    --mount=type=cache,target=/usr/local/cargo/git \
#     cargo build --lib --verbose

# RUN --mount=type=cache,target=/usr/local/cargo/registry \
#     --mount=type=cache,target=/usr/local/cargo/git \
#       cargo build --bin app --verbose

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
