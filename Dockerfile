# syntax=docker/dockerfile:1.7

# ---------------------------------------------------------------------------
# Stage 1: builder
#
# Pinned official Rust image so builds are reproducible. cargo-chef splits
# dependency compilation from source compilation: deps rarely change, so
# caching them speeds up rebuilds when only magna's own code is edited.
# ---------------------------------------------------------------------------
FROM rust:1.83-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.68
WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Build only the dependency graph first, cached by recipe.json contents.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p magna

# Now copy the real sources and build the binary itself.
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
RUN cargo build --release -p magna \
    && strip target/release/magna

# ---------------------------------------------------------------------------
# Stage 2: runtime
#
# debian:bookworm-slim keeps glibc + ca-certificates + tzdata available for
# TLS to Postgres without bundling a build toolchain. Final image lands
# around 90-110 MB depending on dependency footprint.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --user-group --no-create-home --shell /usr/sbin/nologin magna

COPY --from=builder /build/target/release/magna /usr/local/bin/magna

USER magna
EXPOSE 4800
ENV RUST_LOG=info

ENTRYPOINT ["/usr/local/bin/magna"]
