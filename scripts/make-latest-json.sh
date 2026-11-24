#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Prints the Tauri v2 static update manifest (latest.json) for one release, for upload as a
# release asset next to the updater archive. Pure: reads only its arguments, makes no request.
#
# Usage: make-latest-json.sh <version> <notes-file> <sig-file> <asset-url> <pub-date>
#   <sig-file>  the .sig written by `tauri signer sign --app-version <version>` (or by `tauri build`
#               with createUpdaterArtifacts); its contents go into `signature` as they are.
#   <pub-date>  RFC 3339, for example 2026-11-25T00:00:00Z.
set -euo pipefail

if [ $# -ne 5 ]; then
  echo "usage: $0 <version> <notes-file> <sig-file> <asset-url> <pub-date>" >&2
  exit 2
fi
version=$1 notes=$2 sig=$3 url=$4 date=$5

[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "bad version: $version" >&2; exit 2; }
[[ $url == https://* ]] || { echo "asset URL must be https" >&2; exit 2; }
[[ $date =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] || { echo "bad pub-date: $date" >&2; exit 2; }
# The app rejects a signature whose trusted comment does not carry this exact version.
base64 -d <"$sig" | grep -q "^trusted comment: .*	version:$version\$" ||
  { echo "$sig is not signed for version $version (sign with --app-version)" >&2; exit 2; }

jq -n --arg v "$version" --rawfile notes "$notes" --arg date "$date" \
  --arg sig "$(tr -d '[:space:]' <"$sig")" --arg url "$url" \
  '{version: $v, notes: $notes, pub_date: $date,
    platforms: {"darwin-aarch64": {signature: $sig, url: $url}}}'
