#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Prints the body of the `## [<version>]` section of CHANGELOG.md, for the GitHub release notes.
# Fails if the section is missing or has no text.
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: release-notes.sh <version>" >&2; exit 2; }
changelog=$(dirname "$0")/../CHANGELOG.md

# The section ends at the next `## [` heading or at the link definitions Keep a Changelog puts last.
body=$(awk -v h="## [$1]" '
  $0 == h || index($0, h " ") == 1 { on = 1; next }
  on && (/^## \[/ || /^\[[^]]*\]: /) { exit }
  on { print }
' "$changelog" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}')
[ -n "$(printf '%s' "$body" | tr -d '[:space:]')" ] \
  || { echo "release-notes: no '## [$1]' section with text in CHANGELOG.md" >&2; exit 1; }
printf '%s\n' "$body"
