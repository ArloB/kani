# Keep dependency compilation cacheable independently of application source.
FROM rust:bookworm AS chef
WORKDIR /build
RUN cargo install cargo-chef --locked

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    curl \
    unzip \
    cmake \
    perl \
    clang \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . .
# Records only the dependency graph, so this layer's cache key does not move
# when application source changes.
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /build/recipe.json recipe.json
# The cook stage inherits the workspace linker config; see the constraints register.
COPY scripts/fast-linker.sh scripts/fast-linker.sh

# .dockerignore excludes .git/, so the build cannot derive the commit itself.
# Pass it in (docker build --build-arg GIT_SHA=$(git rev-parse --short HEAD))
# or diagnostics and support bundles report an empty build id.
ARG GIT_SHA=""
ENV GIT_SHA=$GIT_SHA

# Use SQLx offline mode — the committed .sqlx/ directory contains pre-generated
# query metadata so the build does not need a live database.
# Run `cargo sqlx prepare --workspace` locally to regenerate after schema changes.
ENV SQLX_OFFLINE=true

# Compile every dependency. This is the expensive layer and it is reused for as
# long as Cargo.lock and the manifests are unchanged.
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

# Build the CLI first so it can provision the frontend toolchain.
RUN cargo build --release -p kani-cli \
    && ./target/release/kani-cli setup

RUN cargo build --release -p kani-web


FROM debian:bookworm-slim AS runtime

# Set INSTALL_KCC=true at build time to include Kindle Comic Converter:
#   docker compose build --build-arg INSTALL_KCC=true
# KCC enables MOBI/AZW3 export via the /rest/chapters/{id}/export/kcc endpoint.
ARG INSTALL_KCC=false

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    curl \
    nodejs \
    && if [ "$INSTALL_KCC" = "true" ]; then \
        apt-get install -y --no-install-recommends python3 python3-pip p7zip-full \
        && pip3 install --no-cache-dir --break-system-packages KindleComicConverter; \
    fi \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 kani && useradd -u 1000 -g kani -d /app -M kani

# The binary embeds the frontend; copying static assets would create a second source of truth.
WORKDIR /app
COPY --from=builder --chown=kani:kani /build/target/release/kani-web ./kani-web
COPY --chown=kani:kani entrypoint.sh ./entrypoint.sh
RUN chmod +x ./entrypoint.sh

# /data holds the database and WASM extensions.
# /library is a separate mount point for the manga image library — it can be
# a bind-mounted path on a different drive/filesystem on the host.
RUN mkdir -p /data /library && chown kani:kani /data /library

USER kani

# Run from /data so that relative paths in the database (./library, ./wasm_sources)
# and the SQLite file (kani.db) all land inside the mounted volume.
WORKDIR /data

EXPOSE 8242

HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
    CMD curl -f http://localhost:8242/health || exit 1

ENV KANI_BIND=0.0.0.0:8242
# KANI_STATIC_DIR is deliberately unset: the binary serves its embedded copy.
# Set it at runtime to override with a bind-mounted directory.
# Library images live in their own volume so they can be on a separate drive/filesystem
ENV KANI_LIBRARY_DIR=/library

CMD ["/app/entrypoint.sh"]
