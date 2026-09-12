#!/bin/bash
# Release-build the crate and its examples.
#
# Usage: ./ci-build.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

cargo build --release --all-targets
