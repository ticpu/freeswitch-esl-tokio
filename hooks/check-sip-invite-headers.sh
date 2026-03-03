#!/bin/bash
# Compare SipInviteHeader enum variants against FreeSWITCH sofia.c.
#
# Usage: check-sip-invite-headers.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Fetched into .freeswitch-src/ from GitHub
#
# Output: last line is "SipInviteHeader <rust_count>/<c_count>"
# Exit: 0 on match, 1 on mismatch

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_FILE="$REPO_ROOT/freeswitch-types/src/variables/sip_invite.rs"

FS_SOURCE="${1:-${FREESWITCH_SOURCE:-}}"
C_FILE=""

if [ -n "$FS_SOURCE" ]; then
	C_FILE="$FS_SOURCE/src/mod/endpoints/mod_sofia/sofia.c"
	if [ ! -f "$C_FILE" ]; then
		echo "error: $C_FILE not found" >&2
		exit 1
	fi
else
	CACHE_DIR="$REPO_ROOT/.freeswitch-src"
	C_FILE="$CACHE_DIR/sofia.c"
	if [ ! -f "$C_FILE" ]; then
		echo "Fetching sofia.c from GitHub..." >&2
		mkdir -p "$CACHE_DIR"
		curl -sL "https://raw.githubusercontent.com/signalwire/freeswitch/master/src/mod/endpoints/mod_sofia/sofia.c" \
			-o "$C_FILE"
	fi
fi

# Extract sip_i_* string literals from sofia.c (hardcoded variable names only,
# not the dynamic unknown-header loop which constructs names at runtime)
c_names=$(grep -oP '"sip_i_[a-z_]+"' "$C_FILE" \
	| tr -d '"' \
	| sort -u)

# Extract wire names from the SipInviteHeader enum definition
rust_names=$(grep -oP '=> "sip_i_[a-z_]+"' "$RUST_FILE" \
	| grep -oP '"sip_i_[a-z_]+"' \
	| tr -d '"' \
	| sort -u)

rust_count=$(echo "$rust_names" | wc -l)
c_count=$(echo "$c_names" | wc -l)

missing_in_rust=$(comm -23 <(echo "$c_names") <(echo "$rust_names"))
extra_in_rust=$(comm -13 <(echo "$c_names") <(echo "$rust_names"))

rc=0

if [ -n "$missing_in_rust" ]; then
	echo "SipInviteHeader missing from Rust (present in sofia.c):"
	echo "$missing_in_rust" | sed 's/^/  + /'
	rc=1
fi

if [ -n "$extra_in_rust" ]; then
	echo "SipInviteHeader has wire names not in sofia.c:"
	echo "$extra_in_rust" | sed 's/^/  - /'
	rc=1
fi

echo "SipInviteHeader ${rust_count}/${c_count}"

exit $rc
