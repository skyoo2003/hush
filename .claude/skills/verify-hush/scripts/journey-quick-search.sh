#!/usr/bin/env bash
# Quick-search journey: global shortcut shows the popup, typing reaches the input, Escape clears and hides it.
# Usage: journey-quick-search.sh   Evidence -> .verify/current/quick-search.{txt,*.png}
set -euo pipefail
source "$(dirname "$0")/lib.sh"
alive || die "no live instance; run launch.sh"
P=$(pid); OUT="$RUN/quick-search.txt"; : > "$OUT"
PROBE=20261122  # digits only: words trigger the macOS autocorrect bubble, which eats the first Escape
state() { swift "$SCRIPTS/windows.swift" "$P" | awk '$1=="quick-search" {print $2}'; }
front() { osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true'; }
log() { echo "$*" | tee -a "$OUT"; }
fail() { log "FAIL: $*"; exit 1; }

[ "$(state)" = 0 ] || fail "quick-search already visible before the shortcut"
log "step 1: Cmd+Shift+Space"
osascript -e 'tell application "System Events" to key code 49 using {command down, shift down}'
sleep 1
[ "$(state)" = 1 ] || fail "popup not shown (another app may own the shortcut: grep 'shortcut not registered' $LOG)"
[ "$(front)" = "$P" ] || fail "popup shown but $APP_SLUG is not frontmost; refusing to type into another app"
log "  popup visible and frontmost"

log "step 2: type probe"
osascript -e "tell application \"System Events\" to keystroke \"$PROBE\""
sleep 0.5
"$SCRIPTS/shot.sh" quick-search quick-search-typed >/dev/null
ax_until "$RUN/quick-search-ax.txt" "desc=Search value=$PROBE" || fail "search field (aria-label=Search) does not hold the probe; see quick-search-ax.txt"
log "  field aria-label=Search value=$PROBE"

log "step 3: Escape"
osascript -e 'tell application "System Events" to key code 53'
sleep 0.7
[ "$(state)" = 0 ] || fail "popup still visible after Escape"
log "  popup hidden"

log "step 4: reopen, field must be empty"
osascript -e 'tell application "System Events" to key code 49 using {command down, shift down}'
sleep 1
[ "$(state)" = 1 ] || fail "popup did not reopen"
ax_until "$RUN/quick-search-reopen-ax.txt" "AXSearchField" || fail "search field not found after reopen"
grep -q "value=$PROBE" "$RUN/quick-search-reopen-ax.txt" && fail "probe still in the field after Escape"
"$SCRIPTS/shot.sh" quick-search quick-search-reopened >/dev/null
osascript -e 'tell application "System Events" to key code 53'
sleep 0.7
[ "$(state)" = 0 ] || fail "popup still visible after final Escape"
log "PASS: quick-search show/type/escape/clear"
