#!/bin/bash
# Compare CallState enum variants against FreeSWITCH switch_channel_callstate_t.
#
# Usage: check-call-states.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Fetched into .freeswitch-src/ from GitHub

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_FILE="$REPO_ROOT/src/channel.rs"

FS_SOURCE="${1:-${FREESWITCH_SOURCE:-}}"
C_FILE=""

if [ -n "$FS_SOURCE" ]; then
	C_FILE="$FS_SOURCE/src/include/switch_types.h"
	if [ ! -f "$C_FILE" ]; then
		echo "error: $C_FILE not found" >&2
		exit 1
	fi
else
	CACHE_DIR="$REPO_ROOT/.freeswitch-src"
	C_FILE="$CACHE_DIR/switch_types.h"
	if [ ! -f "$C_FILE" ]; then
		echo "Fetching switch_types.h from GitHub..." >&2
		mkdir -p "$CACHE_DIR"
		curl -sL "https://raw.githubusercontent.com/signalwire/freeswitch/master/src/include/switch_types.h" \
			-o "$C_FILE"
	fi
fi

# Extract wire names from switch_channel_callstate_t enum, stripping CCS_ prefix
c_names=$(grep -oP '^\tCCS_\K[A-Z_]+' "$C_FILE" \
	| sort)

# Extract wire names from Display impl for CallState
rust_names=$(sed -n '/impl fmt::Display for CallState/,/^    }/p' "$RUST_FILE" \
	| grep -oP '=> "\K[A-Z_]+(?=")' \
	| sort)

missing_in_rust=$(comm -23 <(echo "$c_names") <(echo "$rust_names"))
extra_in_rust=$(comm -13 <(echo "$c_names") <(echo "$rust_names"))

rc=0

if [ -n "$missing_in_rust" ]; then
	echo "CallState missing from Rust (present in switch_channel_callstate_t):"
	echo "$missing_in_rust" | sed 's/^/  + /'
	rc=1
fi

if [ -n "$extra_in_rust" ]; then
	echo "CallState has wire names not in switch_channel_callstate_t:"
	echo "$extra_in_rust" | sed 's/^/  - /'
	rc=1
fi

if [ $rc -eq 0 ]; then
	echo "CallState matches switch_channel_callstate_t"
fi

exit $rc
