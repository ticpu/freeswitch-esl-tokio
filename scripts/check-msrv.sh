#!/bin/bash
# Build the crate on the rust-version it declares so the floor stays true.
#
# Usage: ./check-msrv.sh
#
# The newest releases of several dependencies need a later Rust, so the lock is
# first re-resolved for the declared version: this rewrites Cargo.lock. Installs
# the toolchain through rustup when missing. Run before a release, and in CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

msrv="$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml)"
echo "resolving dependencies for rust $msrv"
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +stable update
rustup toolchain install "$msrv" --profile minimal --no-self-update
cargo "+$msrv" check --locked --all-targets --message-format=short

echo "msrv ok"
