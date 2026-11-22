# Sourced by the verify-locite scripts. Brand values come from brand.rs (rule 9: never hardcode them).
ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
brand() { grep -E "^pub const $1:" "$ROOT/src-tauri/src/brand.rs" | grep -oE '"[^"]*"' | head -1 | tr -d '"'; }
BUNDLE_ID=$(brand BUNDLE_ID)
APP_SLUG=$(brand APP_SLUG)
BIN="$ROOT/src-tauri/target/debug/$APP_SLUG"
DATA_DIR="$HOME/Library/Application Support/$BUNDLE_ID"
LOG_DIR="$HOME/Library/Logs/$BUNDLE_ID"
# The app's log file rotates daily by UTC date.
LOG="$LOG_DIR/$APP_SLUG.$(date -u +%F).log"
RUNS="$ROOT/.verify"
RUN="$RUNS/current"
SCRIPTS="$ROOT/.claude/skills/verify-locite/scripts"
die() { echo "verify-locite: $*" >&2; exit 1; }
pid() { cat "$RUN/pid" 2>/dev/null; }
alive() { local p; p=$(pid) && [ -n "$p" ] && kill -0 "$p" 2>/dev/null; }
# WebKit builds its AX tree lazily, and a busy main thread (setup, keychain prompt) answers nothing,
# so poll. Usage: ax_until <outfile> <grep pattern> [tries]
ax_until() {
  for _ in $(seq 1 "${3:-5}"); do swift "$SCRIPTS/ax.swift" "$(pid)" > "$1" 2>&1; grep -q "$2" "$1" && return 0; sleep 0.5; done; return 1; }
