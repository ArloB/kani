# ─── Stage 1: Builder ────────────────────────────────────────────────────────
#
# Split into a dependency layer and a source layer via cargo-chef. Without it
# `COPY . .` precedes the build, so *any* source change invalidates the layer
# and BoringSSL, SQLite and zstd are all recompiled from scratch — which is
# most of the build time, on every commit, on every architecture.
FROM rust:bookworm AS chef
WORKDIR /build
RUN cargo install cargo-chef --locked

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    curl \
    unzip \
    # boring-sys2 (BoringSSL, pulled in by rquest) needs CMake to configure,
    # Perl to generate its assembly, and libclang for its bindgen step. Without
    # these the image cannot be built at all: the build script panics first with
    # "is `cmake` not installed?", then "Unable to find libclang".
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
# cargo-chef carries .cargo/config.toml into the cook stage, and that config
# points the linker at scripts/fast-linker.sh. Without the script every build
# script fails to link, which is a confusing way to discover the dependency.
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

# Now the workspace itself. Changes here rebuild only first-party crates.
COPY . .

# kani-cli first, then use it to fetch the JS vendor files and build tools.
# build.rs sees PROFILE=release and runs tailwind --minify and esbuild to
# produce static/css/main.css and static/js/dist/.
RUN cargo build --release -p kani-cli \
    && ./target/release/kani-cli setup

# The release binary (build.rs bundles CSS and JS).
RUN cargo build --release -p kani-web


# ─── Stage 2: Runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Set INSTALL_KCC=true at build time to include Kindle Comic Converter:
#   docker compose build --build-arg INSTALL_KCC=true
# KCC enables MOBI/AZW3 export via the /rest/chapters/{id}/export/kcc endpoint.
ARG INSTALL_KCC=false

# Set INSTALL_BROWSER=true at build time to include Chromium + puppeteer-core.
# Required for extensions that use browser-based token capture (e.g. kani-comix).
# Adds ~250 MB to the image; omit if those extensions are not needed.
#   docker compose build --build-arg INSTALL_BROWSER=true
ARG INSTALL_BROWSER=false

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    curl \
    nodejs \
    && if [ "$INSTALL_BROWSER" = "true" ]; then \
        apt-get install -y --no-install-recommends npm chromium; \
    fi \
    && if [ "$INSTALL_KCC" = "true" ]; then \
        apt-get install -y --no-install-recommends python3 python3-pip p7zip-full \
        && pip3 install --no-cache-dir --break-system-packages KindleComicConverter; \
    fi \
    && if [ "$INSTALL_BROWSER" = "true" ]; then \
        npm install -g puppeteer-core && rm -rf /root/.npm; \
    fi \
    && rm -rf /var/lib/apt/lists/*

# Pin the uid/gid rather than letting `useradd -r` pick from the system range.
# It was landing on 999 while docker-compose told users to `chown 1000:1000` to
# fix bind-mount permissions — advice that could not work. 1000 is also the
# first non-system uid on a typical Linux desktop, so a bind mount owned by the
# host user just works with no chown at all.
RUN groupadd -g 1000 kani && useradd -u 1000 -g kani -d /app -M kani

# Copy the compiled binary. The frontend is embedded in it, so there is no
# static directory to ship: the image carried one only because the server used
# to read assets from disk, and keeping both would leave two copies of the same
# files and two ways for them to disagree.
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
