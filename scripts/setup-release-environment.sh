#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# One-time repository setup for .github/workflows/release.yml (ADR-021). Run by the repository admin:
#   bash scripts/setup-release-environment.sh <owner/repo>
# Creates or updates (idempotent):
#   - environment `release`: the admin running this is the required reviewer, deployments only from v* tags;
#   - tag ruleset `release tags`: only the repository admin role may create, move, or delete refs/tags/v*.
# The signing secrets are then added to the `release` environment by hand: APPLE_CERTIFICATE,
# APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY, APPLE_API_KEY_ID, APPLE_API_ISSUER, APPLE_API_KEY
# (ADR-021), and TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD (ADR-022).
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: setup-release-environment.sh <owner/repo>" >&2; exit 2; }
repo=$1
uid=$(gh api user --jq .id)

environment() {
  cat <<JSON
{"reviewers": [{"type": "User", "id": $uid}], "prevent_self_review": false,
 "deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
}
tag_policy='{"name": "v*", "type": "tag"}'
# actor_id 5 is the built-in repository admin role.
ruleset='{"name": "release tags", "target": "tag", "enforcement": "active",
  "conditions": {"ref_name": {"include": ["refs/tags/v*"], "exclude": []}},
  "rules": [{"type": "creation"}, {"type": "update"}, {"type": "deletion"}],
  "bypass_actors": [{"actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always"}]}'

environment | gh api -X PUT "repos/$repo/environments/release" --input - >/dev/null
echo "environment release: required reviewer $uid, custom deployment policies"

policies=repos/$repo/environments/release/deployment-branch-policies
if gh api "$policies" --jq '.branch_policies[] | select(.type == "tag" and .name == "v*") | .id' | grep -q .; then
  echo "deployment policy v* (tag): exists"
else
  gh api -X POST "$policies" --input - <<<"$tag_policy" >/dev/null
  echo "deployment policy v* (tag): created"
fi

id=$(gh api "repos/$repo/rulesets?includes_parents=false" --jq '.[] | select(.name == "release tags") | .id')
if [ -n "$id" ]; then
  gh api -X PUT "repos/$repo/rulesets/$id" --input - <<<"$ruleset" >/dev/null
  echo "ruleset release tags: updated ($id)"
else
  gh api -X POST "repos/$repo/rulesets" --input - <<<"$ruleset" >/dev/null
  echo "ruleset release tags: created"
fi
