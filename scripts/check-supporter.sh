#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The self-reported supporter flag unlocks nothing (absolute rule 8): outside tests, only the
# files below may name it. commands.rs stores and returns it, src/ipc/Settings.ts is its
# generated type, and SponsorPanel.tsx shows the switch and the thank-you badge. src/dev (the UI
# harness mock, never in a build: CI greps dist/ for it) stores and returns it like commands.rs (ADR-023).
set -euo pipefail
cd "$(dirname "$0")/.."

ALLOWED=(src-tauri/src/commands.rs src/ipc/Settings.ts src/components/SponsorPanel.tsx)

hits=""
# Rust: only the code above the first `#[cfg(test)]` counts, and only the bare word (not `set_supporter`).
while IFS= read -r f; do
  if awk '/^#\[cfg\(test\)\]/ {exit} /(^|[^A-Za-z0-9_])supporter([^A-Za-z0-9_]|$)/ {found=1} END {exit !found}' "$f"; then
    hits+="$f"$'\n'
  fi
done < <(find src-tauri/src -name '*.rs')
hits+=$(grep -rlw supporter src --include='*.ts' --include='*.tsx' --exclude='*.test.ts' --exclude='*.test.tsx' | grep -v '^src/dev/' || true)

bad=""
while IFS= read -r f; do
  [ -z "$f" ] && continue
  ok=0
  for a in "${ALLOWED[@]}"; do [ "$f" = "$a" ] && ok=1; done
  [ "$ok" = 1 ] || bad+="$f"$'\n'
done <<<"$hits"

if [ -n "$bad" ]; then
  printf '%s' "$bad"
  echo "check-supporter: settings.supporter is read outside the thank-you badge; it must unlock nothing" >&2
  exit 1
fi
echo "check-supporter: ok"
