#!/bin/bash
# List the CodeQL results the SARIF filter removed, as Markdown.
#
# Usage: ./ci-codeql-summary.sh LANGUAGE UNFILTERED.sarif FILTERED.sarif
#
# Filtered results never reach code scanning, so this job summary is the only
# record of what .github/codeql/sarif-filter.txt hid on a given run.

set -euo pipefail

language="$1"
unfiltered="$2"
filtered="$3"

jq -rn --arg language "$language" --slurpfile a "$unfiltered" --slurpfile b "$filtered" '
	def results($sarif):
		[$sarif[0].runs[].results[]
			| .locations[0].physicalLocation as $loc
			| "\(.ruleId) \($loc.artifactLocation.uri):\($loc.region.startLine)"];
	(results($a) - results($b)) as $removed
	| "### CodeQL \($language): \($removed | length) result(s) filtered\n",
		($removed[] | "- `\(.)`")
'
