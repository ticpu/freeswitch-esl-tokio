#!/bin/bash
# Compare EslEventType enum variants against FreeSWITCH C ESL EVENT_NAMES[].
#
# Usage: check-event-types.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Cloned into .freeswitch-src/ (fetches esl_event.c from GitHub)
#
# Output: last line is "EslEventType <rust_count>/<c_count>"
# Exit: 0 on match, 1 on mismatch

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_FILE="$REPO_ROOT/freeswitch-types/src/event.rs"

FS_SOURCE="${1:-${FREESWITCH_SOURCE:-}}"
C_FILE=""

if [ -n "$FS_SOURCE" ]; then
	C_FILE="$FS_SOURCE/libs/esl/src/esl_event.c"
	if [ ! -f "$C_FILE" ]; then
		echo "error: $C_FILE not found" >&2
		exit 1
	fi
else
	CACHE_DIR="$REPO_ROOT/.freeswitch-src"
	C_FILE="$CACHE_DIR/esl_event.c"
	if [ ! -f "$C_FILE" ]; then
		echo "Fetching esl_event.c from GitHub..." >&2
		mkdir -p "$CACHE_DIR"
		curl -sL "https://raw.githubusercontent.com/signalwire/freeswitch/master/libs/esl/src/esl_event.c" \
			-o "$C_FILE"
	fi
fi

# Extract C wire names from EVENT_NAMES[] array, excluding ALL (subscription pseudo-type)
c_names=$(sed -n '/^static.*EVENT_NAMES\[\]/,/^};/p' "$C_FILE" \
	| grep -oP '"\K[A-Z_]+(?=")' \
	| grep -v '^ALL$' \
	| sort)

# Extract Rust wire names from Display impl, stopping at the ALL arm
rust_names=$(sed -n '/impl fmt::Display for EslEventType/,/=> "ALL"/p' "$RUST_FILE" \
	| grep -oP '=> "\K[A-Z_]+(?=")' \
	| grep -v '^ALL$' \
	| sort)

rust_count=$(echo "$rust_names" | wc -l)
c_count=$(echo "$c_names" | wc -l)

missing_in_rust=$(comm -23 <(echo "$c_names") <(echo "$rust_names"))
extra_in_rust=$(comm -13 <(echo "$c_names") <(echo "$rust_names"))

rc=0

if [ -n "$missing_in_rust" ]; then
	echo "EslEventType missing from C ESL:"
	echo "$missing_in_rust" | sed 's/^/  + /'
	rc=1
fi

if [ -n "$extra_in_rust" ]; then
	echo "EslEventType has wire names not in C ESL:"
	echo "$extra_in_rust" | sed 's/^/  - /'
	rc=1
fi

echo "EslEventType ${rust_count}/${c_count}"

exit $rc
