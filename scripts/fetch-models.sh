#!/bin/sh
# Fetches pinned embedding model files into models/{id}/ (git-ignored except manifest.json)
# and checks each against the tracked manifest. Developer tooling only: the app never runs it.
# Idempotent: files that already match are not downloaded again.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)/models"

fetch() { # id repo commit
  for f in config.json tokenizer.json model.safetensors; do
    want=$(sed -n "s/.*\"$f\": \"\([0-9a-f]*\)\".*/\1/p" "$root/$1/manifest.json")
    out="$root/$1/$f"
    if [ -f "$out" ] && echo "$want  $out" | shasum -a 256 -c - >/dev/null 2>&1; then
      continue
    fi
    curl -fsSL -o "$out.part" "https://huggingface.co/$2/resolve/$3/$f"
    echo "$want  $out.part" | shasum -a 256 -c - >/dev/null || { rm -f "$out.part"; echo "fetch-models: $1/$f checksum mismatch" >&2; exit 2; }
    mv "$out.part" "$out"
  done
  echo "fetch-models: $1 ok"
}

fetch bge-small-en-v1.5 BAAI/bge-small-en-v1.5 5c38ec7c405ec4b44b94cc5a9bb96e735b38267a
fetch multilingual-e5-small intfloat/multilingual-e5-small 614241f622f53c4eeff9890bdc4f31cfecc418b3
