#!/usr/bin/env bash
# Cargo runner (src-tauri/.cargo/config.toml) and launcher for dev builds: re-sign the binary with a stable
# identity, then exec it. Ad-hoc signatures change on every rebuild, so the login keychain asks for the
# DB key again each time. With LOCITE_DEV_SIGN_IDENTITY set (a codesigning identity name or SHA-1 from
# `security find-identity -v -p codesigning`), the identity stays the same and one "Always Allow" sticks.
# Unset: runs the binary unchanged.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: dev-sign.sh <binary> [args...]" >&2; exit 2; }
if [ -n "${LOCITE_DEV_SIGN_IDENTITY:-}" ] && [ "$(uname)" = Darwin ]; then
  brand=$(dirname "$0")/../src-tauri/src/brand.rs
  id=$(grep -E '^pub const BUNDLE_ID:' "$brand" | grep -oE '"[^"]*"' | tr -d '"')
  [ -n "$id" ] || { echo "dev-sign: no BUNDLE_ID in $brand" >&2; exit 2; }
  codesign --force --sign "$LOCITE_DEV_SIGN_IDENTITY" --identifier "$id" "$1" 2>/dev/null \
    || { echo "dev-sign: codesign failed for identity '$LOCITE_DEV_SIGN_IDENTITY'" >&2; exit 1; }
fi
exec "$@"
