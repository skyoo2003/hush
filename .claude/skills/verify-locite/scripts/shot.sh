#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Capture one Locite window (not the whole screen) into the current run dir.
# Usage: shot.sh <main|settings|quick-search> <name>   -> .verify/current/<name>.png
set -euo pipefail
source "$(dirname "$0")/lib.sh"
[ $# -eq 2 ] || die "usage: shot.sh <main|settings|quick-search> <name>"
alive || die "no live instance; run launch.sh"
id=$(swift "$SCRIPTS/windows.swift" "$(pid)" | awk -v l="$1" '$1==l {print $3; exit}')
[ -n "$id" ] || die "no $1 window"
screencapture -x -o -l "$id" "$RUN/$2.png"
echo "$RUN/$2.png"
