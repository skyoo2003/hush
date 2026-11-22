## Why

<!-- The problem this solves. Link the issue: Closes #123 -->

## What changed

## How it was verified

## Checklist

- [ ] Follows every absolute rule in `CLAUDE.md` (no network egress of document data, `ScopedPath` reads, single encrypted DB, no private data in logs).
- [ ] Tests are in this change. A tier-gated feature tests rejection on Free and acceptance on permitted tiers.
- [ ] CI checks pass locally, including `cargo deny check licenses`.
- [ ] New dependencies are recorded in `docs/decisions.md` with reason, license, and network use, and the license is GPL-3.0-compatible.
- [ ] `docs/progress.md` is updated.
- [ ] Every commit is signed off (`git commit -s`) under the DCO.
