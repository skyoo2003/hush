#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Keeps signing secrets out of reach of untrusted code (ADR-021): only release.yml may name secrets
# (other than GITHUB_TOKEN) or an environment, no workflow may run on pull_request_target or
# workflow_run, and release.yml runs on pushed tags only.
set -euo pipefail
cd "$(dirname "$0")/../.github/workflows"

fail=0
bad() { echo "check-workflows: $1" >&2; fail=1; }

for f in *.yml *.yaml; do
  [ -f "$f" ] || continue
  code=$(sed 's/#.*//' "$f") # comments may mention anything
  if grep -nE 'pull_request_target|workflow_run' <<<"$code"; then bad "$f uses pull_request_target or workflow_run"; fi
  [ "$f" = release.yml ] && continue
  if grep -nE 'secrets' <<<"${code//secrets.GITHUB_TOKEN/}"; then bad "$f names a secret"; fi
  if grep -nE '^[[:space:]-]*environment[[:space:]]*:' <<<"$code"; then bad "$f names an environment"; fi
done

# The whole `on:` block of release.yml, comments and blank lines dropped, must be exactly this.
want=$'on:\n  push:\n    tags: [\'v*\']'
got=$(sed 's/[[:space:]]*#.*//' release.yml | grep -v '^[[:space:]]*$' |
  awk '/^on:/ {on = 1; print; next} on && /^[^ ]/ {exit} on {print}')
[ "$got" = "$want" ] || bad "release.yml must trigger on push tags ['v*'] only; found:"$'\n'"$got"

[ "$fail" -eq 0 ] || exit 1
echo "check-workflows: ok"
