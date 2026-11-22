#!/usr/bin/env bash
# Air-Gap journey: flip the main-window switch, relaunch, check it persisted, then restore it the same way.
# Usage: journey-air-gap.sh   (needs a live instance from launch.sh; relaunches twice with --no-build)
# Evidence -> the starting run dir: air-gap.txt, air-gap-*.png, air-gap-*-ax.txt
set -euo pipefail
source "$(dirname "$0")/lib.sh"
alive || die "no live instance; run launch.sh"
EV=$(readlink "$RUN"); OUT="$EV/air-gap.txt"; : > "$OUT"
log() { echo "$*" | tee -a "$OUT"; }
fail() { log "FAIL: $*"; exit 1; }
SWITCH="AXCheckBox/AXSwitch title=Air-Gap mode value="
# Prints 1 (ON) or 0 (OFF) as the UI shows it, saving the AX dump as evidence.
value() { ax_until "$EV/air-gap-$1-ax.txt" "$SWITCH" 10 || fail "switch not found; see air-gap-$1-ax.txt"
  grep -o "${SWITCH}[01]" "$EV/air-gap-$1-ax.txt" | tail -c 2; }
shot() { "$SCRIPTS/shot.sh" main "air-gap-$1" >/dev/null && cp "$RUN/air-gap-$1.png" "$EV/" 2>/dev/null || true; }
relaunch() { "$SCRIPTS/cleanup.sh" >/dev/null; "$SCRIPTS/launch.sh" --no-build >/dev/null || fail "relaunch failed"; }
flip() { # $1 = step name, $2 = expected new value
  swift "$SCRIPTS/press.swift" "$(pid)" AXCheckBox "Air-Gap mode" || fail "press failed"
  for _ in 1 2 3 4 5; do [ "$(value "$1")" = "$2" ] && break; sleep 0.5; done
  [ "$(value "$1")" = "$2" ] || fail "switch did not change to $2 after press"
  grep -q "role=alert\|Could not" "$EV/air-gap-$1-ax.txt" && fail "error shown after press"
  shot "$1"; log "  $1: switch shows $2"
}

orig=$(value start); flipped=$(( 1 - orig ))
log "start: switch shows $orig"; shot start
log "step 1: press the switch"; flip flipped "$flipped"
log "step 2: relaunch, the flipped value must persist"; relaunch
[ "$(value relaunched)" = "$flipped" ] || fail "after relaunch the switch shows $(value relaunched), not $flipped"
shot relaunched; log "  relaunched: switch shows $flipped"
log "step 3: press again to restore"; flip restored "$orig"
log "step 4: relaunch, the original value must persist"; relaunch
[ "$(value final)" = "$orig" ] || fail "after the restore relaunch the switch shows $(value final), not $orig"
shot final; log "  final: switch shows $orig"
log "PASS: air-gap toggle/persist/restore ($orig -> $flipped -> $orig)"
