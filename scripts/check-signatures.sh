#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Lists every Mach-O in an app bundle and fails if any lacks a valid signature. Found by `file`, not by
# name, so a newly bundled library is checked without editing this script.
# --developer-id: each must also be signed by a Developer ID Application identity with the hardened
# runtime, and the app must pass Gatekeeper and carry a stapled notarization ticket.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: check-signatures.sh <App.app> [--developer-id]" >&2; exit 2; }
app=${1%/}
devid=false
[ "${2:-}" = --developer-id ] && devid=true

fail=0
count=0
while IFS= read -r f; do
  file -b "$f" | grep -q 'Mach-O' || continue
  count=$((count + 1))
  echo "$f"
  if ! codesign --verify --strict "$f"; then echo "  FAIL: invalid or missing signature" >&2; fail=1; continue; fi
  if $devid; then
    info=$(codesign -dvv "$f" 2>&1)
    grep -q '^Authority=Developer ID Application' <<<"$info" || { echo "  FAIL: not signed with Developer ID" >&2; fail=1; }
    grep -qE '^CodeDirectory .*flags=[^ ]*runtime' <<<"$info" || { echo "  FAIL: no hardened runtime" >&2; fail=1; }
  fi
done < <(find "$app/Contents" -type f)
[ "$count" -gt 0 ] || { echo "check-signatures: no Mach-O found in $app" >&2; exit 1; }

codesign --verify --strict "$app" || { echo "FAIL: app bundle signature" >&2; fail=1; }
if $devid; then
  spctl -a -vv -t exec "$app" || fail=1
  xcrun stapler validate "$app" || fail=1
fi
[ "$fail" -eq 0 ] || { echo "check-signatures: FAILED" >&2; exit 1; }
echo "check-signatures: ok ($count Mach-O files)"
