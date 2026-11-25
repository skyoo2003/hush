#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Signs a built app bundle without an Apple identity, so it is not notarized (ADR-021).
# Usage: sign-macos.sh <App.app> [out-dir]
#   SIGN_IDENTITY  unset or - : ad-hoc, for your own builds (docs/building.md).
#                  SHA-1 of the release's self-signed certificate: official releases. The identity
#                  stays the same across versions, so the keychain keeps trusting updates. Its
#                  keychain must be in the user search list (codesign --keychain does not find it).
#   out-dir  also writes <out-dir>/<APP_NAME>_<version>_aarch64.dmg and the updater archive
#            <out-dir>/<App>.app.tar.gz (ADR-022), both from the signed app.
# No hardened runtime: library validation refuses libraries without an Apple team ID.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: sign-macos.sh <App.app> [out-dir]" >&2; exit 2; }
app=${1%/}
[ -d "$app/Contents/MacOS" ] || { echo "sign-macos: $app is not an app bundle" >&2; exit 2; }
flags=(--force --sign "${SIGN_IDENTITY:--}")

# Nested Mach-O files deepest first, then the bundle, which signs its main executable (Apple advises
# against --deep for signing). Signing the main executable by path would sign the bundle too early.
main=$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
find "$app/Contents" -type f | awk -F/ '{print NF "\t" $0}' | sort -rn | cut -f2- |
  while IFS= read -r f; do
    if [ "$f" != "$main" ] && file -b "$f" | grep -q 'Mach-O'; then codesign "${flags[@]}" "$f"; fi
  done
codesign "${flags[@]}" "$app"
echo "sign-macos: signed $app with identity ${SIGN_IDENTITY:--}"
[ $# -ge 2 ] || exit 0

out=$2
mkdir -p "$out"
brand=$(dirname "$0")/../src-tauri/src/brand.rs
name=$(grep -E '^pub const APP_NAME:' "$brand" | grep -oE '"[^"]*"' | tr -d '"')
[ -n "$name" ] || { echo "sign-macos: no APP_NAME in $brand" >&2; exit 2; }
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/stage"
ditto "$app" "$work/stage/$(basename "$app")"
ln -s /Applications "$work/stage/Applications"
hdiutil create -volname "$name" -srcfolder "$work/stage" -fs HFS+ -format UDZO -ov "$out/${name}_${version}_aarch64.dmg"
COPYFILE_DISABLE=1 tar -czf "$out/$(basename "$app").tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
echo "sign-macos: wrote $out/${name}_${version}_aarch64.dmg and $out/$(basename "$app").tar.gz"
