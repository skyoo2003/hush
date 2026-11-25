#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds OUT_DIR/models-kit-VERSION.zip for offline installs (docs/offline-install.md): every
# embedding model in models/catalog.json marked redistributable, its license from models/licenses/,
# manifest.json, and SHA256SUMS over the kit's files. The app checks each model file against its
# own built-in catalog on import, not against this kit's manifest or sums.
# Usage: scripts/make-models-kit.sh VERSION OUT_DIR
set -eu

[ $# -eq 2 ] || { echo "usage: $0 VERSION OUT_DIR" >&2; exit 2; }
version=$1
repo="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$2"
out="$(cd "$2" && pwd)/models-kit-$version.zip"
name="models-kit-$version"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
kit="$stage/$name"
mkdir "$kit"

# manifest.json, and one "id name url sha256" line per file to fetch.
python3 -I - "$repo/models/catalog.json" "$version" "$kit/manifest.json" >"$stage/files" <<'PY'
import json, sys
catalog, version, manifest = sys.argv[1:]
models = [m for m in json.load(open(catalog))["models"]
          if m["kind"] == "embedding" and m["redistributable"]]
if not models:
    sys.exit("make-models-kit: no redistributable embedding model in the catalog")
json.dump({
    "format": 1,
    "version": version,
    "models": [{
        "id": m["id"], "kind": m["kind"], "license": m["license"],
        "licenseFile": m["id"] + "/LICENSE.txt",
        "files": [{k: f[k] for k in ("name", "size", "sha256")} for f in m["files"]],
    } for m in models],
}, open(manifest, "w"), indent=2)
for m in models:
    for f in m["files"]:
        print(m["id"], f["name"], f["url"], f["sha256"])
PY

while read -r id file url sha; do
  mkdir -p "$kit/$id"
  license="$repo/models/licenses/$id.txt"
  [ -f "$license" ] || { echo "make-models-kit: $license is missing" >&2; exit 1; }
  cp "$license" "$kit/$id/LICENSE.txt"
  # A copy fetched by scripts/fetch-models.sh is reused if it matches.
  if [ -f "$repo/models/$id/$file" ]; then cp "$repo/models/$id/$file" "$kit/$id/$file"; fi
  if ! echo "$sha  $kit/$id/$file" | shasum -a 256 -c - >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$kit/$id/$file" "$url"
    echo "$sha  $kit/$id/$file" | shasum -a 256 -c - >/dev/null ||
      { echo "make-models-kit: $id/$file checksum mismatch" >&2; exit 1; }
  fi
done <"$stage/files"

(cd "$kit" && find . -type f | sed 's|^\./||' | LC_ALL=C sort |
  while read -r f; do shasum -a 256 "$f"; done >"$stage/sums")
mv "$stage/sums" "$kit/SHA256SUMS"
(cd "$kit" && shasum -a 256 -c SHA256SUMS >/dev/null)
rm -f "$out"
# Stored, not deflated: weights barely compress, and the import reads them faster.
(cd "$stage" && find "$name" -type f | LC_ALL=C sort | zip -q -0 -X "$out" -@)
echo "make-models-kit: $out"
