#!/usr/bin/env bash
# Fails if a brand string from src-tauri/src/brand.rs is hardcoded outside brand.rs / src/brand.ts.
set -euo pipefail
cd "$(dirname "$0")/.."

BRAND=src-tauri/src/brand.rs
TARGETS=(src-tauri/src src src-tauri/capabilities index.html)

# Value of `pub const NAME: &str = "value";` (first string literal on the line; for arrays, every literal).
values() {
  local v
  v=$(grep -E "^pub const $1:" "$BRAND" | grep -oE '"[^"]*"' | tr -d '"') || true
  # An empty result would make the check pass vacuously.
  [ -n "$v" ] || { echo "check-brand: no value for $1 in $BRAND" >&2; exit 2; }
  echo "$v"
}

hits=""
scan() { # scan <grep flags...> -e <pattern>
  hits+=$(grep -rnI "$@" "${TARGETS[@]}" | grep -vE "^($BRAND|src/brand\.ts):")$'\n' || true
}

for name in APP_NAME APP_SLUG; do
  for v in $(values "$name"); do scan -w -F -e "$v"; done
done
for name in BUNDLE_ID KEYCHAIN_SERVICE SITE_URL SPONSOR_URLS; do
  for v in $(values "$name"); do scan -F -e "$v"; done
done
for v in $(values SITE_URL); do
  host=${v#*://}
  scan -F -e "${host%%/*}"
done
for v in $(values PACK_EXT); do
  scan -F -e ".$v" -e "\"$v\""
done

hits=$(printf '%s' "$hits" | grep -v '^$' | sort -u) || true
if [ -n "$hits" ]; then
  echo "$hits"
  echo "check-brand: hardcoded brand strings found; use brand.rs / src/brand.ts" >&2
  exit 1
fi
echo "check-brand: ok"
