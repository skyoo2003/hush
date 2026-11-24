#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Signs a built app bundle. Usage: sign-macos.sh <App.app> [out-dir]
#
# SIGN_IDENTITY=-   Ad-hoc signs the app in place (no hardened runtime, no notarization). For your own
#                   builds (docs/building.md); out-dir is not used.
# SIGN_IDENTITY=<Developer ID Application identity>
#                   Signs every Mach-O inside-out with the hardened runtime and a timestamp, notarizes
#                   and staples the app, then builds, signs, notarizes, and staples
#                   <out-dir>/<APP_NAME>_<version>_aarch64.dmg. Needs NOTARY_KEY (path to the App Store
#                   Connect .p8), NOTARY_KEY_ID, and NOTARY_ISSUER. SKIP_NOTARIZE=1 stops after signing
#                   the app (local hardened-runtime test with a development identity).
#
# No entitlements file: the app runs under the hardened runtime and library validation without one.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: sign-macos.sh <App.app> [out-dir]" >&2; exit 2; }
app=${1%/}
[ -d "$app/Contents/MacOS" ] || { echo "sign-macos: $app is not an app bundle" >&2; exit 2; }
id=${SIGN_IDENTITY:?set SIGN_IDENTITY to - (ad-hoc) or a Developer ID Application identity}

flags=(--force --sign "$id")
[ "$id" = - ] || flags+=(--timestamp --options runtime)

# Nested Mach-O files deepest first, then the bundle, which signs its main executable (Apple advises
# against --deep for signing). Signing the main executable by path would sign the bundle too early.
main=$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
find "$app/Contents" -type f | awk -F/ '{print NF "\t" $0}' | sort -rn | cut -f2- |
  while IFS= read -r f; do
    if [ "$f" != "$main" ] && file -b "$f" | grep -q 'Mach-O'; then codesign "${flags[@]}" "$f"; fi
  done
codesign "${flags[@]}" "$app"
echo "sign-macos: signed $app with identity $id"
[ "$id" = - ] && exit 0
[ "${SKIP_NOTARIZE:-}" = 1 ] && exit 0

out=${2:?out-dir is required with a real identity}
mkdir -p "$out"
: "${NOTARY_KEY:?}" "${NOTARY_KEY_ID:?}" "${NOTARY_ISSUER:?}"
brand=$(dirname "$0")/../src-tauri/src/brand.rs
name=$(grep -E '^pub const APP_NAME:' "$brand" | grep -oE '"[^"]*"' | tr -d '"')
[ -n "$name" ] || { echo "sign-macos: no APP_NAME in $brand" >&2; exit 2; }
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

notarize() { # notarize <file>: waits for the verdict, fails with the log unless Accepted.
  local result status sub
  result=$(xcrun notarytool submit "$1" --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" \
    --issuer "$NOTARY_ISSUER" --wait --output-format json) || true
  status=$(plutil -extract status raw -o - - <<<"$result" 2>/dev/null) || status=unknown
  if [ "$status" != Accepted ]; then
    echo "sign-macos: notarization of $(basename "$1") ended with status $status" >&2
    echo "$result" >&2
    if sub=$(plutil -extract id raw -o - - <<<"$result" 2>/dev/null); then
      xcrun notarytool log "$sub" --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" >&2 || true
    fi
    exit 1
  fi
  echo "sign-macos: notarized $(basename "$1")"
}

# 1. The app itself, so its ticket is stapled and Gatekeeper can check it offline (Air-Gap users).
ditto -c -k --keepParent "$app" "$work/app.zip"
notarize "$work/app.zip"
xcrun stapler staple "$app"

# 2. The DMG that ships it.
mkdir "$work/stage"
ditto "$app" "$work/stage/$(basename "$app")"
ln -s /Applications "$work/stage/Applications"
dmg="$out/${name}_${version}_aarch64.dmg"
hdiutil create -volname "$name" -srcfolder "$work/stage" -fs HFS+ -format UDZO -ov "$dmg"
codesign --force --timestamp --sign "$id" "$dmg"
notarize "$dmg"
xcrun stapler staple "$dmg"
echo "sign-macos: wrote $dmg"
