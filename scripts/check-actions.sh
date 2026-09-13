#!/bin/bash
# Fail when a GitHub Action pin is not the latest release it claims to be.
#
# Usage: ./check-actions.sh
#
# Every uses: must read owner/repo[/path]@<commit sha> # <release tag>, the tag
# must still resolve to that commit, and it must be the action's latest release.
# Needs gh. Run before a release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

USES='^([^:]+:[0-9]+):[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*([^@[:space:]]+)@([^[:space:]#]+)[[:space:]]*(#[[:space:]]*([^[:space:]]+))?'

failed=0
fail() {
	echo "actions: $*" >&2
	failed=1
}

declare -A latest_release=()

while IFS= read -r line; do
	if [[ ! $line =~ $USES ]]; then
		fail "unparsed uses: line $line"
		continue
	fi
	where="${BASH_REMATCH[1]}"
	action="${BASH_REMATCH[3]}"
	ref="${BASH_REMATCH[4]}"
	tag="${BASH_REMATCH[6]}"

	case "$action" in
	./* | docker://*) continue ;;
	esac
	repo="$(cut -d/ -f1-2 <<<"$action")"

	if [[ ! $ref =~ ^[0-9a-f]{40}$ ]]; then
		fail "$where: $action@$ref is not pinned to a commit"
		continue
	fi
	if [ -z "$tag" ]; then
		fail "$where: $action@$ref carries no release tag comment"
		continue
	fi

	if ! tag_sha="$(gh api "repos/$repo/commits/$tag" --jq .sha)"; then
		fail "$where: $repo has no tag $tag"
		continue
	fi
	if [ "$tag_sha" != "$ref" ]; then
		fail "$where: $action pins $ref but $tag is $tag_sha"
	fi

	if [ -z "${latest_release[$repo]+set}" ]; then
		# Not releases/latest: codeql-action marks its CodeQL bundle as latest.
		latest_release[$repo]="$(
			gh api "repos/$repo/releases?per_page=100" --jq '.[]
				| select((.draft or .prerelease) | not)
				| .tag_name
				| select(test("^v?[0-9]+(\\.[0-9]+)*$"))' |
				sort -V | tail -1
		)"
	fi
	latest="${latest_release[$repo]}"
	if [ -z "$latest" ]; then
		fail "$where: $repo publishes no versioned release"
	elif [ "$tag" != "$latest" ]; then
		fail "$where: $action is at $tag, latest release is $latest"
	fi
done < <(grep -Hn -E '^[[:space:]]*(-[[:space:]]+)?uses:' .github/workflows/*.yml)

[ "$failed" -eq 0 ] && echo "actions ok"
exit "$failed"
