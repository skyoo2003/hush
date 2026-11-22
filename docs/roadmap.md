# Roadmap

Twelve weeks. The repository goes public in week 4, v1.0 ships in week 8, and weeks 9 to 12 add features.

## How the work runs

1. The shared rules live in [CLAUDE.md](../CLAUDE.md) at the repository root.
2. Prompts run in order, each in a fresh session. Commit when the prompt's definition of done (DoD) passes.
3. At the end of each week, run a security review of that week's changes. When an external pull request arrives, run a PR review against the absolute rules.
4. After the repository is public, one hour each week goes to triaging issues and pull requests.
5. When a week runs late, cut from the end of that week's cut line first.

## Weeks

| Week | Theme | Main deliverables |
| --- | --- | --- |
| 1 | Skeleton and secure storage | Encrypted DB, AirGapGuard, ScopedPath |
| 2 | Document parsers and chunker | PDF coordinate extraction, OCR, SHA-256 change detection |
| 3 | Search engine | Local embeddings, hybrid search, eval |
| 4 | Watcher, queue, public release prep | Incremental indexing, LICENSE, CONTRIBUTING, SECURITY, repository goes public |
| 5 | Quick-search UI | Overlay, copy citation, open source file |
| 6 | Models, scheduler, verification | Model manager, slow down during search, Air-Gap proof |
| 7 | Sponsorship and distribution | Sponsor UX, signing and notarization, offline kit, tag source archive on each release (7-B) |
| 8 | v1.0 launch | Hardening, About screen with license notice and source link (8-A), landing page, Show HN |
| 9 | Richer search | Single-question Q&A, filter chips, PII warning and masking |
| 10 | Compare and history | UC-05 change history, UC-01 compare view |
| 11 | Trust and sharing | UC-06 evidence and conflicts, UC-08 `.locitepack`, UC-03 reranker |
| 12 | Document understanding | UC-02 layout OCR and jump to coordinates, UC-07 related documents |

## Decisions due before the repository goes public (week 4)

- **Name.** Renamed from Hush to Locite (ADR-012). Still to do: a trademark search, and renaming the GitHub repository from `skyoo2003/hush`. See [TRADEMARK.md](../TRADEMARK.md).
- **Relicensing.** Contributions come in under the DCO with no CLA, so copyright stays with each contributor, and any later relicensing or dual licensing needs every contributor's consent. Today there is one contributor. If that option should stay open, decide on a CLA before accepting outside contributions.
- **Sponsorship accounts.** Check fees and Korean payout and tax conditions, then open the accounts and fill in `.github/FUNDING.yml` and the goals in [funding.md](funding.md).
