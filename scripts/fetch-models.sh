#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Fetches the embedding models in models/catalog.json into models/{id}/ (git-ignored) and checks
# each file's sha256 against the catalog. Developer tooling only: the app installs models itself.
# Idempotent: files that already match are not downloaded again.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)/models"

# One "id name url sha256" line per embedding model file.
python3 -I -c '
import json, sys
for m in json.load(open(sys.argv[1]))["models"]:
    if m["kind"] == "embedding":
        for f in m["files"]:
            print(m["id"], f["name"], f["url"], f["sha256"])
' "$root/catalog.json" | while read -r id name url sha; do
  out="$root/$id/$name"
  mkdir -p "$root/$id"
  if [ -f "$out" ] && echo "$sha  $out" | shasum -a 256 -c - >/dev/null 2>&1; then
    continue
  fi
  curl -fsSL -o "$out.part" "$url"
  echo "$sha  $out.part" | shasum -a 256 -c - >/dev/null || { rm -f "$out.part"; echo "fetch-models: $id/$name checksum mismatch" >&2; exit 2; }
  mv "$out.part" "$out"
done
echo "fetch-models: ok"
