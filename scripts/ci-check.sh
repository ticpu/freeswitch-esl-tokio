#!/bin/bash
# Type-check and lint the crate.
#
# Usage: ./ci-check.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

cargo check --all-targets
cargo clippy --all-targets -- -D warnings
