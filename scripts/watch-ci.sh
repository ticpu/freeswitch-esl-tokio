#!/bin/bash
# Wait for one workflow's run on one commit, and fail if it fails.
#
# Usage: ./watch-ci.sh [ref] [workflow]      (default: HEAD ci.yml)
#
# Two things this exists to get right:
#
#   - The run is selected by workflow AND commit SHA. The CodeQL workflow is a
#     separate run on the same commit and usually finishes first -- so "the
#     most recent run on the branch" is regularly the scan, not CI.
#   - `gh run watch` exits 0 on a run that failed unless it is given
#     --exit-status.
#
# A run does not exist the instant a push returns, so the id is polled for.

set -euo pipefail

POLL_TIMEOUT=120
POLL_INTERVAL=5

ref="${1:-HEAD}"
workflow="${2:-ci.yml}"
# ^{commit} or an annotated tag resolves to the tag object, which no run
# matches: the release tag is always annotated.
sha="$(git rev-parse "${ref}^{commit}")"

run_id=""
waited=0
while [ -z "$run_id" ]; do
	run_id="$(gh run list --workflow "$workflow" --commit "$sha" --limit 1 \
		--json databaseId --jq '.[0].databaseId // empty')"
	[ -n "$run_id" ] && break
	if [ "$waited" -ge "$POLL_TIMEOUT" ]; then
		echo "no $workflow run appeared for $ref ($sha) within ${POLL_TIMEOUT}s" >&2
		echo "push it first, or check that the workflow triggers on this ref" >&2
		exit 1
	fi
	sleep "$POLL_INTERVAL"
	waited=$((waited + POLL_INTERVAL))
done

echo "watching $workflow run $run_id for $ref ($sha)"
gh run watch --exit-status "$run_id"
