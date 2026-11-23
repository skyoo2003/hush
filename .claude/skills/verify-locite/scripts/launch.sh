#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the debug binary (frontend embedded, no Vite port) and launch it on the user's real data.
# Usage: launch.sh [--no-build]   Prints the run dir on success.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

# Locite has no per-instance data dir: a second copy would share the real DB, keychain key and shortcut.
if pgrep -x "$APP_SLUG" >/dev/null; then
  die "refusing: $APP_SLUG already running (pid $(pgrep -x "$APP_SLUG" | tr '\n' ' ')). Ask the user to quit it."
fi
if [ "${1:-}" != "--no-build" ]; then
  mkdir -p "$RUNS"
  (cd "$ROOT" && pnpm tauri build --debug --no-bundle) > "$RUNS/build.log" 2>&1 || die "build failed; see $RUNS/build.log"
fi
[ -x "$BIN" ] || die "no binary at $BIN"

dir="$RUNS/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$dir"
ln -sfn "$dir" "$RUN"
mkdir -p "$LOG_DIR"; touch "$LOG"
wc -l < "$LOG" | tr -d ' ' > "$dir/log-start"
# dev-sign.sh re-signs with $LOCITE_DEV_SIGN_IDENTITY (if set) so the keychain stops re-prompting, then execs.
"$ROOT/scripts/dev-sign.sh" "$BIN" > "$dir/stdout.txt" 2>&1 &
echo $! > "$dir/pid"

for _ in $(seq 1 240); do
  new=$(tail -n +"$(( $(cat "$dir/log-start") + 1 ))" "$LOG")
  if grep -q "storage open failed" <<<"$new"; then die "storage open failed (see $LOG); run cleanup.sh"; fi
  # "app started" is logged before storage opens; the rendered heading only answers AX once setup returned.
  if grep -q "app started" <<<"$new" && ax_until "$dir/launch-ax.txt" "AXHeading title=$(brand APP_NAME)" 1; then
    echo "$dir"; exit 0
  fi
  alive || die "app exited during startup; see $dir/stdout.txt"
  sleep 0.5
done
screencapture -x "$dir/launch-timeout.png"
die "not ready after 120s. A keychain 'allow access' prompt is the usual cause (see $dir/launch-timeout.png): ask the user to click it. Never click keychain dialogs yourself."
