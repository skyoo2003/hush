#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stop the instance this run started (by recorded pid only). Evidence in .verify/<run>/ is kept.
set -uo pipefail
source "$(dirname "$0")/lib.sh"
p=$(pid) || { echo "cleanup: no run"; exit 0; }
if [ -n "$p" ] && kill -0 "$p" 2>/dev/null && [ "$(ps -o comm= -p "$p")" = "$BIN" ]; then
  kill "$p"; for _ in $(seq 1 20); do kill -0 "$p" 2>/dev/null || break; sleep 0.25; done
  kill -0 "$p" 2>/dev/null && kill -9 "$p"
  echo "cleanup: stopped pid $p"
else echo "cleanup: pid ${p:-?} not ours or already gone"; fi
: > "$RUN/pid"
echo "evidence kept in $(readlink "$RUN")"
