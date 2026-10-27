#!/bin/sh
# Resolve the linker from the active Rust toolchain, without a machine-specific path.
set -eu
rust_host=$(rustc -vV | sed -n 's/^host: //p')
rust_sysroot=$(rustc --print sysroot)
exec "$rust_sysroot/lib/rustlib/$rust_host/bin/rust-lld" "$@"
