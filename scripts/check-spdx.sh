#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Fails if a tracked source file lacks the GPL-3.0-or-later SPDX header in its first 3 lines.
# `--fix` inserts the header (after a shebang, if any). Generated files are skipped.
set -euo pipefail
cd "$(dirname "$0")/.."

ID="SPDX-License-Identifier: GPL-3.0-or-later"
fix=false
[ "${1:-}" = "--fix" ] && fix=true

missing=0
while IFS= read -r f; do
  head -n 3 "$f" | grep -qF "$ID" && continue
  if ! $fix; then
    echo "$f"
    missing=1
    continue
  fi
  case "$f" in *.sh) line="# $ID" ;; *) line="// $ID" ;; esac
  tmp=$(mktemp)
  if head -n 1 "$f" | grep -q '^#!'; then
    { head -n 1 "$f"; echo "$line"; tail -n +2 "$f"; } >"$tmp"
  else
    { echo "$line"; cat "$f"; } >"$tmp"
  fi
  cat "$tmp" >"$f" # keeps the file's mode
  rm -f "$tmp"
  echo "fixed: $f"
done < <(git ls-files -- '*.rs' '*.ts' '*.tsx' '*.js' '*.mjs' '*.swift' '*.sh' \
  ':!src/ipc/*' ':!src/brand.ts' ':!*.d.ts')

if [ "$missing" -ne 0 ]; then
  echo "check-spdx: files above lack '$ID'; run scripts/check-spdx.sh --fix" >&2
  exit 1
fi
echo "check-spdx: ok"
