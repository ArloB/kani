#!/bin/sh
# Linker driver that transparently uses the fastest available linker and falls
# back gracefully, so the workspace builds with or without mold/lld installed —
# neither is required. Wired in as the linux linker via .cargo/config.toml.
# Order: mold (fastest) → lld → the compiler's default linker.
if command -v clang >/dev/null 2>&1; then CC=clang; else CC=cc; fi
if command -v mold >/dev/null 2>&1; then
  exec "$CC" -fuse-ld=mold "$@"
elif command -v ld.lld >/dev/null 2>&1; then
  exec "$CC" -fuse-ld=lld "$@"
else
  exec "$CC" "$@"
fi
