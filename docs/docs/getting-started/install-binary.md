# Install from Binary

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Download

Pre-built binaries are attached to each [GitHub Release](https://github.com/ArloB/kani/releases).

| Platform | Filename |
|----------|---------|
| Linux x86-64 | `kani-x86_64-unknown-linux-musl.tar.gz` |
| Linux aarch64 | `kani-aarch64-unknown-linux-musl.tar.gz` |

## Configure

<!-- TODO: document kani.toml / environment variable configuration for bare-metal -->

## Run as a systemd service

<!-- TODO: example unit file -->

## Build from source

```bash
git clone https://github.com/ArloB/kani.git
cd kani
cargo build --release
```

The binary is at `target/release/kani-web`.
