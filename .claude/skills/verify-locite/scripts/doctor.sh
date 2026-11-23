#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Read-only: is the instance from the current run worth driving? Exit 0 = yes.
set -uo pipefail
source "$(dirname "$0")/lib.sh"
ok=1
say() { printf '%-12s %s\n' "$1" "$2"; }
if alive; then say process "pid $(pid) up ($(ps -o comm= -p "$(pid)"))"; else say process "NOT RUNNING (no live pid in $RUN/pid)"; ok=0; fi
others=$(pgrep -x "$APP_SLUG" | grep -vx "$(pid)" | tr '\n' ' ')
[ -z "$others" ] || { say others "FOREIGN $APP_SLUG pids: $others (not ours; do not drive or kill)"; ok=0; }
if [ -n "$(find "$ROOT/src" "$ROOT/src-tauri/src" -newer "$BIN" -type f 2>/dev/null | head -1)" ]; then
  say build "STALE: sources newer than $BIN; relaunch without --no-build"; ok=0
else say build "binary up to date"; fi
new=$(tail -n +"$(( $(cat "$RUN/log-start" 2>/dev/null || echo 0) + 1 ))" "$LOG" 2>/dev/null)
if grep -q "app started" <<<"$new"; then say log "app started"; else say log "NO 'app started' since launch"; ok=0; fi
if grep -qE " (ERROR|WARN) " <<<"$new"; then say log "$(grep -E ' (ERROR|WARN) ' <<<"$new" | tail -1)"; fi
if osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' >/dev/null 2>&1; then
  say a11y "System Events reachable"; else say a11y "NO accessibility permission for this terminal"; ok=0; fi
alive && swift "$SCRIPTS/windows.swift" "$(pid)" | sed 's/^/windows      /'
[ "$ok" = 1 ] && echo "doctor: ok" || { echo "doctor: NOT ok"; exit 1; }
