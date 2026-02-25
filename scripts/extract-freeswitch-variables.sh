#!/bin/bash
# Extract all channel variable names from FreeSWITCH source code, grouped by module.
#
# Usage: ./extract-freeswitch-variables.sh [/path/to/freeswitch/src]
#
# Output format: module_name:variable_name (one per line, sorted, deduplicated)
# For core files (switch_*.c), the module is the source filename without extension.
# For modules (src/mod/), the module is the directory name (e.g. mod_sofia).

set -euo pipefail

FS_SRC="${1:-../freeswitch/src}"

if [ ! -d "$FS_SRC" ]; then
    echo "error: FreeSWITCH source directory not found: $FS_SRC" >&2
    echo "usage: $0 [/path/to/freeswitch/src]" >&2
    exit 1
fi

grep -Pro 'switch_channel_[gs]et_variable\([^,]+,\s*"\K[^"]+' "$FS_SRC/" \
    | sed -rne '
        # Module files: src/mod/<category>/<mod_name>/file.c:var -> mod_name:var
        s,^.*/src/mod/[^/]+/([^/]+)/[^:]+:(.+),\1:\2,p
        # Core files: src/switch_foo.c:var -> switch_foo:var
        s,^.*/src/(switch_[^/.]+)\.[^:]+:(.+),\1:\2,p
    ' \
    | sort -u
