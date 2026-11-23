## Why

<!-- The problem this solves. Link the issue: Closes #123 -->

## What changed

## How it was verified

## Checklist

- [ ] Follows every absolute rule in `CLAUDE.md` (no network egress of document data, `ScopedPath` reads, single encrypted DB, no private data in logs).
- [ ] Tests are in this change.
- [ ] Network code: this change does not touch `net.rs`, `AirGapGuard`, `NetPurpose`, or any outbound request. If it does, the description explains why and how the Air-Gap rules still hold.
- [ ] Dependencies: this change adds none. If it does, each one is listed below with its license, its license is GPL-3.0-compatible, and it is recorded in `docs/decisions.md` with reason and network use.
  <!-- name — license — reason — network use -->
- [ ] Adds no feature lock, usage limit, payment or license code, or telemetry.
- [ ] CI checks pass locally, including `cargo deny check` and `scripts/check-spdx.sh`.
- [ ] Every commit is signed off (`git commit -s`) under the DCO.
