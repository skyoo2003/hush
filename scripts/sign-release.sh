#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Signs an official release with the self-signed release certificate (ADR-021), then writes the DMG and
# the updater archive. Usage: sign-release.sh <App.app> <out-dir>
#   RELEASE_SIGNING_P12           base64 of the .p12 holding the certificate and its private key
#   RELEASE_SIGNING_P12_PASSWORD  its password
# The certificate must be packaging/macos/release-signing-cert.pem. The private key lives only in a
# temporary keychain, which is deleted on exit along with its entry in the user keychain search list.
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: sign-release.sh <App.app> <out-dir>" >&2; exit 2; }
: "${RELEASE_SIGNING_P12:?}" "${RELEASE_SIGNING_P12_PASSWORD:?}"
here=$(cd "$(dirname "$0")" && pwd)
want=$(/usr/bin/openssl x509 -in "$here/../packaging/macos/release-signing-cert.pem" -noout -fingerprint -sha1 |
  cut -d= -f2 | tr -d :)

work=$(mktemp -d)
kc=$work/release.keychain-db
pw=$(/usr/bin/openssl rand -hex 24)
orig=$(security list-keychains -d user | tr -d '"')
cleanup() {
  # shellcheck disable=SC2086 # one word per keychain path; keychain paths here have no spaces
  security list-keychains -d user -s $orig
  security delete-keychain "$kc" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

security create-keychain -p "$pw" "$kc"
security set-keychain-settings -lut 3600 "$kc"
security unlock-keychain -p "$pw" "$kc"
(umask 077; printf '%s' "$RELEASE_SIGNING_P12" | base64 --decode > "$work/cert.p12")
security import "$work/cert.p12" -k "$kc" -f pkcs12 -P "$RELEASE_SIGNING_P12_PASSWORD" -T /usr/bin/codesign >/dev/null
rm -f "$work/cert.p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" >/dev/null
# codesign finds an identity only through the search list; --keychain alone does not find it.
# shellcheck disable=SC2086
security list-keychains -d user -s "$kc" $orig

got=$(security find-identity -p codesigning "$kc" | awk '$1 ~ /^[0-9]+\)$/ {print $2; exit}')
[ "$got" = "$want" ] || { echo "sign-release: the secret's certificate ($got) is not release-signing-cert.pem ($want)" >&2; exit 1; }

SIGN_IDENTITY=$want bash "$here/sign-macos.sh" "$1" "$2"
bash "$here/check-signatures.sh" "$1"
# The keychain trusts updates through this requirement; it must name the certificate, not a cdhash.
codesign -d -r- "$1" 2>&1 | grep -qi "certificate leaf = H\"$want\"" ||
  { echo "sign-release: designated requirement does not name the release certificate" >&2; exit 1; }
echo "sign-release: signed with certificate $want"
