#!/bin/bash
# Verify that all declared variable enum variants exist in FreeSWITCH source code.
#
# Usage: ./check-variable-coverage.sh [/path/to/freeswitch/src]
#
# Extracts wire-format strings from ChannelVariable and SofiaVariable enum
# definitions, then checks each one appears in the FreeSWITCH source grep output.
# Reports stale/typo variants and optionally missing coverage.

set -euo pipefail

FS_SRC="${1:-../freeswitch/src}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"

if [ ! -d "$FS_SRC" ]; then
    echo "warning: FreeSWITCH source not found at $FS_SRC, skipping check" >&2
    exit 0
fi

# Extract all variable names from FS source (just the variable names, no module prefix)
fs_vars=$(grep -Proh 'switch_channel_[gs]et_variable\([^,]+,\s*"\K[^"]+' "$FS_SRC/" | sort -u)

# Extract wire-format strings from our enum definitions (the "quoted_string" after =>)
extract_enum_wires() {
    grep -Po '=> "\K[^"]+' "$1" | sort -u
}

core_wires=$(extract_enum_wires "$CRATE_DIR/src/variables/core.rs")
sofia_wires=$(extract_enum_wires "$CRATE_DIR/src/variables/sofia.rs")

exit_code=0

echo "Checking ChannelVariable variants against FreeSWITCH source..."
while IFS= read -r wire; do
    if ! echo "$fs_vars" | grep -qxF "$wire"; then
        echo "  MISSING in FS source: $wire"
        exit_code=1
    fi
done <<< "$core_wires"

echo "Checking SofiaVariable variants against FreeSWITCH source..."
while IFS= read -r wire; do
    if ! echo "$fs_vars" | grep -qxF "$wire"; then
        echo "  MISSING in FS source: $wire"
        exit_code=1
    fi
done <<< "$sofia_wires"

if [ "$exit_code" -eq 0 ]; then
    echo "All declared variants found in FreeSWITCH source."
fi

exit "$exit_code"
