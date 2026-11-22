#!/bin/sh
# Model-check AirGap.tla: each config × each invariant, compared with the expected results below.
# Usage: verification/airgap/run.sh [-v]   (-v prints TLC's counterexample for every FAIL)
# Exit 0 only when every result matches EXPECTED. Needs Java 11+; fetches a pinned TLC once.
set -eu

TLA_VERSION=1.7.4
TLA_SHA256=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
JAR="${XDG_CACHE_HOME:-$HOME/.cache}/hush-tla/tla2tools-$TLA_VERSION.jar"

# config:invariant:expected. A FAIL here is a known finding, not a pass.
EXPECTED="
AirGap:CheckedOnlyWhenOff:PASS
AirGap:NoSendAfterOnAck:FAIL
AirGap:OnSurvivesRestart:PASS
AirGapAsync:CheckedOnlyWhenOff:PASS
AirGapAsync:NoSendAfterOnAck:FAIL
AirGapAsync:OnSurvivesRestart:FAIL
"

here=$(cd "$(dirname "$0")" && pwd)
verbose=${1:-}

if [ ! -f "$JAR" ]; then
  mkdir -p "$(dirname "$JAR")"
  curl -fsSL -o "$JAR.part" "https://github.com/tlaplus/tlaplus/releases/download/v$TLA_VERSION/tla2tools.jar"
  mv "$JAR.part" "$JAR"
fi
echo "$TLA_SHA256  $JAR" | shasum -a 256 -c - >/dev/null || { echo "tla2tools.jar checksum mismatch: $JAR" >&2; exit 2; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$here/AirGap.tla" "$work/"

status=0
for line in $EXPECTED; do
  cfg=${line%%:*}; rest=${line#*:}; inv=${rest%%:*}; want=${rest#*:}
  # Same constants, one invariant, so TLC reports every invariant, not only the first to break.
  sed '/^INVARIANTS/,$d' "$here/$cfg.cfg" > "$work/$cfg-$inv.cfg"
  printf 'INVARIANT %s\n' "$inv" >> "$work/$cfg-$inv.cfg"
  out="$work/$cfg-$inv.out"
  (cd "$work" && java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC -workers auto -cleanup \
     -metadir "$work/meta-$cfg-$inv" -config "$cfg-$inv.cfg" AirGap.tla) > "$out" 2>&1 || true
  if grep -q "Model checking completed. No error has been found" "$out"; then got=PASS
  elif grep -q "Invariant $inv is violated" "$out"; then got=FAIL
  else got=ERROR; fi
  states=$(grep -o '[0-9,]* distinct states found' "$out" | tail -1 | cut -d' ' -f1)
  mark=ok; [ "$got" = "$want" ] || { mark=UNEXPECTED; status=1; }
  printf '%-12s %-20s %-5s (expected %s, %s states) %s\n' "$cfg" "$inv" "$got" "$want" "${states:-?}" "$mark"
  if [ "$got" = ERROR ] || { [ "$got" = FAIL ] && [ -n "$verbose" ]; } || [ "$mark" = UNEXPECTED ]; then
    sed -n '/^Error:/,/distinct states found/p' "$out"
    [ "$got" = ERROR ] && tail -20 "$out"
  fi
done
exit $status
