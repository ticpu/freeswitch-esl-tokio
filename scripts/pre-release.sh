#!/bin/bash
# Everything that must pass before tagging and publishing a release.
#
# Usage: ./pre-release.sh
#
# Requires a live FreeSWITCH ESL listener for the live_freeswitch suite, the
# x86_64-pc-windows-msvc target for the cross-check, rustup for the MSRV
# toolchain, and cargo-semver-checks.
# Traced (set -x) so a failure names the gate that stopped it.

set -euxo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

cargo fmt
cargo clippy --release --all-targets -- -D warnings
cargo test --release
cargo test --test live_freeswitch -- --ignored
cargo build --release --all-targets
cargo check --all-targets --target x86_64-pc-windows-msvc
cargo semver-checks check-release
cargo publish --dry-run
# Last: it re-resolves Cargo.lock for the declared rust-version, and every gate
# above should run against the newest dependencies.
"$SCRIPT_DIR/check-msrv.sh"
