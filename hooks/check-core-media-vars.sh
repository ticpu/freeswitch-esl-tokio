#!/bin/bash
# Compare CoreMediaVariable enum variants against FreeSWITCH set_stats() in switch_core_media.c.
#
# Usage: check-core-media-vars.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Fetched into .freeswitch-src/ from GitHub
#
# Output: last line is "CoreMediaVariable <rust_count>/<c_count>"
# Exit: 0 on match, 1 on mismatch

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_FILE="$REPO_ROOT/freeswitch-types/src/variables/core_media.rs"

FS_SOURCE="${1:-${FREESWITCH_SOURCE:-}}"
C_FILE=""

if [ -n "$FS_SOURCE" ]; then
	C_FILE="$FS_SOURCE/src/switch_core_media.c"
	if [ ! -f "$C_FILE" ]; then
		echo "error: $C_FILE not found" >&2
		exit 1
	fi
else
	CACHE_DIR="$REPO_ROOT/.freeswitch-src"
	C_FILE="$CACHE_DIR/switch_core_media.c"
	if [ ! -f "$C_FILE" ]; then
		echo "Fetching switch_core_media.c from GitHub..." >&2
		mkdir -p "$CACHE_DIR"
		curl -sL "https://raw.githubusercontent.com/signalwire/freeswitch/master/src/switch_core_media.c" \
			-o "$C_FILE"
	fi
fi

# Extract stat suffixes from set_stats() — lines like: add_stat(..., "in_raw_bytes");
# Also extract the prefixes from the call sites: set_stats(session, ..., "audio");
prefixes=$(grep -oP 'set_stats\(session,\s*\S+,\s*"\K[^"]+' "$C_FILE" | sort -u)
suffixes=$(sed -n '/^static void set_stats/,/^}/p' "$C_FILE" \
	| grep -oP 'add_stat(?:_double)?\([^,]+,\s*"\K[^"]+' \
	| sort -u)

# Build expected variable names: rtp_{prefix}_{suffix}
c_names=""
for prefix in $prefixes; do
	for suffix in $suffixes; do
		c_names="${c_names}rtp_${prefix}_${suffix}"$'\n'
	done
done
c_names=$(echo "$c_names" | grep -v '^$' | sort)

# Extract wire names from CoreMediaVariable enum
rust_names=$(grep -oP '=> "\K[^"]+' "$RUST_FILE" | sort)

rust_count=$(echo "$rust_names" | wc -l)
c_count=$(echo "$c_names" | wc -l)

missing_in_rust=$(comm -23 <(echo "$c_names") <(echo "$rust_names"))
extra_in_rust=$(comm -13 <(echo "$c_names") <(echo "$rust_names"))

rc=0

if [ -n "$missing_in_rust" ]; then
	echo "CoreMediaVariable missing from Rust (present in set_stats()):"
	echo "$missing_in_rust" | sed 's/^/  + /'
	rc=1
fi

if [ -n "$extra_in_rust" ]; then
	echo "CoreMediaVariable has wire names not in set_stats():"
	echo "$extra_in_rust" | sed 's/^/  - /'
	rc=1
fi

echo "CoreMediaVariable ${rust_count}/${c_count}"

exit $rc
