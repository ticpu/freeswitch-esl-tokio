#!/bin/bash
# Fail when a GitHub Action pin is not the release it claims, carries a known
# advisory, or has missed a release Dependabot should already have delivered.
#
# Usage: ./check-actions.sh
#
# Every uses: must read owner/repo[/path]@<commit sha> # <release tag>, the tag
# must still resolve to that commit, and no GitHub advisory may affect it. A
# newer release fails once it is GRACE_DAYS old and warns before that.
# Needs gh. Run before a release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(dirname "$SCRIPT_DIR")"
cd "$CRATE_DIR"

# .github/dependabot.yml's cooldown plus its weekly schedule, and a few days to merge.
GRACE_DAYS=14
cutoff="$(date -u -d "-$GRACE_DAYS days" +%Y-%m-%dT%H:%M:%SZ)"

USES='^([^:]+:[0-9]+):[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*([^@[:space:]]+)@([^[:space:]#]+)[[:space:]]*(#[[:space:]]*([^[:space:]]+))?'

failed=0
fail() {
	echo "actions: $*" >&2
	failed=1
}
warn() {
	echo "actions: warning: $*" >&2
}

# Whether version $1 sorts after version $2.
newer() {
	[ "$1" != "$2" ] && [ "$(printf '%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]
}

declare -A releases=()

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

	advisories="$(
		gh api -X GET /advisories -f ecosystem=actions -f "affects=$repo@$tag,$action@$tag" \
			--jq '.[] | "\(.ghsa_id) \(.severity): \(.summary)"'
	)"
	if [ -n "$advisories" ]; then
		fail "$where: $action@$tag is affected by $advisories"
	fi

	if [ -z "${releases[$repo]+set}" ]; then
		# Not releases/latest: codeql-action marks its CodeQL bundle as latest.
		releases[$repo]="$(
			gh api "repos/$repo/releases?per_page=100" --jq '.[]
				| select((.draft or .prerelease) | not)
				| select(.tag_name | test("^v?[0-9]+(\\.[0-9]+)*$"))
				| "\(.tag_name) \(.published_at)"'
		)"
	fi
	if [ -z "${releases[$repo]}" ]; then
		fail "$where: $repo publishes no versioned release"
		continue
	fi
	latest="$(cut -d' ' -f1 <<<"${releases[$repo]}" | sort -V | tail -1)"
	due="$(awk -v cutoff="$cutoff" '$2 <= cutoff { print $1 }' <<<"${releases[$repo]}" | sort -V | tail -1)"
	if [ -n "$due" ] && newer "$due" "$tag"; then
		fail "$where: $action is at $tag, $due has been out over $GRACE_DAYS days"
	elif newer "$latest" "$tag"; then
		warn "$where: $action is at $tag, $latest is under $GRACE_DAYS days old"
	fi
done < <(grep -Hn -E '^[[:space:]]*(-[[:space:]]+)?uses:' .github/workflows/*.yml)

[ "$failed" -eq 0 ] && echo "actions ok"
exit "$failed"
