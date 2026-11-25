#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Lists every Mach-O in an app bundle and fails if any lacks a valid signature. Found by `file`, not by
# name, so a newly bundled library is checked without editing this script.
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: check-signatures.sh <App.app>" >&2; exit 2; }
app=${1%/}

fail=0
count=0
while IFS= read -r f; do
  file -b "$f" | grep -q 'Mach-O' || continue
  count=$((count + 1))
  echo "$f"
  if ! codesign --verify --strict "$f"; then echo "  FAIL: invalid or missing signature" >&2; fail=1; continue; fi
done < <(find "$app/Contents" -type f)
[ "$count" -gt 0 ] || { echo "check-signatures: no Mach-O found in $app" >&2; exit 1; }

codesign --verify --strict "$app" || { echo "FAIL: app bundle signature" >&2; fail=1; }
[ "$fail" -eq 0 ] || { echo "check-signatures: FAILED" >&2; exit 1; }
echo "check-signatures: ok ($count Mach-O files)"
