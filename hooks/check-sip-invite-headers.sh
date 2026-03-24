#!/bin/bash
# Verify that all sip_i_* wire names in FreeSWITCH sofia.c have a matching
# SipHeader variant in the sip-header crate.
#
# With SipPassthroughHeader, sip_i_* names are computed dynamically from
# SipHeader variants. This hook ensures no new headers were added to
# sofia.c that sip-header doesn't know about.
#
# Usage: check-sip-invite-headers.sh [path-to-freeswitch-source]
#
# The FreeSWITCH source is located via (in order):
#   1. First argument
#   2. $FREESWITCH_SOURCE env var
#   3. Fetched into .freeswitch-src/ from GitHub
#
# Exit: 0 on match, 1 on mismatch

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

# Find sip-header source: check cargo registry cache
SIP_HEADER_SRC=$(find ~/.cargo/registry/src -path '*/sip-header-*/src/header.rs' \
	-printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)

if [ -z "$SIP_HEADER_SRC" ]; then
	echo "error: sip-header source not found in cargo registry" >&2
	exit 1
fi

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

# Reverse the sip_i_ transformation to get SIP header names:
# "sip_i_call_info" → "call-info" → check if SipHeader has it
# SipHeader uses eq_ignore_ascii_case, so we extract canonical names
sip_header_names=$(grep -oP '=> "[A-Za-z][A-Za-z0-9-]+"' "$SIP_HEADER_SRC" \
	| grep -oP '"[^"]+"' \
	| tr -d '"' \
	| sort -u)

# Build a set of expected sip_i_ wire names from SipHeader canonical names
expected_wire_names=""
for name in $sip_header_names; do
	wire=$(echo "$name" | tr '[:upper:]' '[:lower:]' | tr '-' '_')
	expected_wire_names="$expected_wire_names
sip_i_$wire"
done
expected_wire_names=$(echo "$expected_wire_names" | sed '/^$/d' | sort -u)

c_count=$(echo "$c_names" | wc -l)
sip_count=$(echo "$expected_wire_names" | wc -l)

missing_in_sip_header=$(comm -23 <(echo "$c_names") <(echo "$expected_wire_names"))

rc=0

if [ -n "$missing_in_sip_header" ]; then
	echo "sip_i_* names in sofia.c with no matching SipHeader variant:"
	echo "$missing_in_sip_header" | sed 's/^/  + /'
	rc=1
fi

echo "SipHeader covers ${c_count} C-side sip_i_* names (${sip_count} SipHeader variants)"

exit $rc
