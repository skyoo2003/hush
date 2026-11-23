#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Fails unless every non-merge commit in <base>..<head> has a Signed-off-by trailer with its author's email.
# Usage: scripts/check-dco.sh <base> [head]
set -euo pipefail
base=${1:?usage: check-dco.sh <base> [head]}
head=${2:-HEAD}

# A bad range must fail here, not inside the for list where set -e cannot see it.
commits=$(git rev-list --no-merges "$base..$head")
bad=0
for c in $commits; do
  email=$(git log -1 --format='%ae' "$c")
  if ! git log -1 --format='%(trailers:key=Signed-off-by,valueonly)' "$c" | grep -qF "<$email>"; then
    echo "$(git log -1 --format='%h %s' "$c"): no Signed-off-by for <$email>"
    bad=1
  fi
done

if [ "$bad" -ne 0 ]; then
  echo "check-dco: sign off with 'git commit -s' (or 'git rebase --signoff $base' for existing commits)" >&2
  exit 1
fi
echo "check-dco: ok"
