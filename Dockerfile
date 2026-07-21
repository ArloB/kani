# ─── Stage 1: Builder ────────────────────────────────────────────────────────
FROM rust:bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Copy the full workspace
COPY . .

# .dockerignore excludes .git/, so the build cannot derive the commit itself.
# Pass it in (docker build --build-arg GIT_SHA=$(git rev-parse --short HEAD))
# or diagnostics and support bundles report an empty build id.
ARG GIT_SHA=""
ENV GIT_SHA=$GIT_SHA

# Use SQLx offline mode — the committed .sqlx/ directory contains pre-generated
# query metadata so the build does not need a live database.
# Run `cargo sqlx prepare --workspace` locally to regenerate after schema changes.
ENV SQLX_OFFLINE=true

# Build kani-cli first, then use it to fetch JS vendor files and build tools.
# build.rs detects the PROFILE=release and automatically runs tailwind --minify
# and esbuild to produce static/css/main.css and static/js/dist/.
RUN cargo build --release -p kani-cli \
    && ./target/release/kani-cli setup

# Build the release binary (triggers build.rs which bundles CSS and JS).
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

RUN groupadd -r kani && useradd -r -g kani -d /app kani

# Copy the compiled binary and static web assets (CSS and JS already built by build.rs).
WORKDIR /app
COPY --from=builder --chown=kani:kani /build/target/release/kani-web ./kani-web
COPY --from=builder --chown=kani:kani /build/static/ ./static/
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
# Static assets are at the fixed /app/static path regardless of working directory
ENV KANI_STATIC_DIR=/app/static
# Library images live in their own volume so they can be on a separate drive/filesystem
ENV KANI_LIBRARY_DIR=/library

CMD ["/app/entrypoint.sh"]
