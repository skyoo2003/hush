#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Fails if a brand string from src-tauri/src/brand.rs is hardcoded outside brand.rs / src/brand.ts.
set -euo pipefail
cd "$(dirname "$0")/.."

BRAND=src-tauri/src/brand.rs
TARGETS=(src-tauri/src src src-tauri/capabilities index.html)

# String literals of `pub const NAME ... ;`, which may span lines (rustfmt wraps long arrays).
values() {
  local v
  v=$(awk -v n="$1" '$0 ~ "^pub const " n ":" {on=1} on {print} on && /;/ {exit}' "$BRAND" |
    grep -oE '"[^"]*"' | tr -d '"') || true
  # An empty result would make the check pass vacuously.
  [ -n "$v" ] || { echo "check-brand: no value for $1 in $BRAND" >&2; exit 2; }
  echo "$v"
}

hits=""
scan() { # scan <grep flags...> -e <pattern>
  hits+=$(grep -rnI "$@" "${TARGETS[@]}" | grep -vE "^($BRAND|src/brand\.ts):")$'\n' || true
}

# `vals=$(values ...) || exit 2`: an exit inside $(...) alone would not stop the script.
for name in APP_NAME APP_SLUG; do
  vals=$(values "$name") || exit 2
  for v in $vals; do scan -w -F -e "$v"; done
done
for name in BUNDLE_ID KEYCHAIN_SERVICE REPO_URL SITE_URL UPDATE_MANIFEST_URL SPONSOR_URLS; do
  vals=$(values "$name") || exit 2
  for v in $vals; do scan -F -e "$v"; done
done
vals=$(values SITE_URL) || exit 2
for v in $vals; do
  host=${v#*://}
  scan -F -e "${host%%/*}"
done
vals=$(values PACK_EXT) || exit 2
for v in $vals; do
  scan -F -e ".$v" -e "\"$v\""
done

hits=$(printf '%s' "$hits" | grep -v '^$' | sort -u) || true
if [ -n "$hits" ]; then
  echo "$hits"
  echo "check-brand: hardcoded brand strings found; use brand.rs / src/brand.ts" >&2
  exit 1
fi
echo "check-brand: ok"
