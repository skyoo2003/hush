#!/bin/sh
# Fetches the pinned PDFium binary into src-tauri/vendor/pdfium/ (git-ignored).
# Developer and build tooling only: the app itself never downloads anything.
# Idempotent: skips the download when the marker matches the pinned hash.
set -eu

RELEASE=chromium/8125
ASSET=pdfium-mac-univ.tgz
SHA256=0cf06803b9a4acc489fc967a1b05990622ca1cf41fa027ff9cc89eeab9b19279

dest="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/vendor/pdfium"
marker="$dest/.sha256"

if [ -f "$marker" ] && [ "$(cat "$marker")" = "$SHA256" ] && [ -f "$dest/lib/libpdfium.dylib" ]; then
  echo "fetch-pdfium: up to date"
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL -o "$work/$ASSET" "https://github.com/bblanchon/pdfium-binaries/releases/download/$(echo "$RELEASE" | sed 's|/|%2F|')/$ASSET"
echo "$SHA256  $work/$ASSET" | shasum -a 256 -c - >/dev/null || { echo "fetch-pdfium: checksum mismatch" >&2; exit 2; }

rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$work/$ASSET" -C "$dest"
echo "$SHA256" > "$marker"
echo "fetch-pdfium: installed $RELEASE into $dest"
