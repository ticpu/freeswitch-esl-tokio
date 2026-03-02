#!/bin/bash
# Compare ChannelState enum variants against FreeSWITCH switch_channel_state_t.
#
# Usage: check-channel-states.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Fetched into .freeswitch-src/ from GitHub
#
# Output: last line is "ChannelState <rust_count>/<c_count>"
# Exit: 0 on match, 1 on mismatch

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_FILE="$REPO_ROOT/freeswitch-types/src/channel.rs"

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

# Extract wire names from switch_channel_state_t enum (tab-indented to skip comments)
c_names=$(grep -oP '^\tCS_[A-Z_]+' "$C_FILE" \
	| tr -d '\t' \
	| sort)

# Extract wire names from Display impl for ChannelState
rust_names=$(sed -n '/impl fmt::Display for ChannelState/,/^    }/p' "$RUST_FILE" \
	| grep -oP '=> "\KCS_[A-Z_]+(?=")' \
	| sort)

rust_count=$(echo "$rust_names" | wc -l)
c_count=$(echo "$c_names" | wc -l)

missing_in_rust=$(comm -23 <(echo "$c_names") <(echo "$rust_names"))
extra_in_rust=$(comm -13 <(echo "$c_names") <(echo "$rust_names"))

rc=0

if [ -n "$missing_in_rust" ]; then
	echo "ChannelState missing from Rust (present in switch_channel_state_t):"
	echo "$missing_in_rust" | sed 's/^/  + /'
	rc=1
fi

if [ -n "$extra_in_rust" ]; then
	echo "ChannelState has wire names not in switch_channel_state_t:"
	echo "$extra_in_rust" | sed 's/^/  - /'
	rc=1
fi

echo "ChannelState ${rust_count}/${c_count}"

exit $rc
