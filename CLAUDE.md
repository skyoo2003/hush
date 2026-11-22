# Hush — Agent Rules

## Product in one line

A Zero-Cloud RAG desktop app that indexes, searches, and summarizes documents in user-chosen local folders 100% on-device, and attaches the original source (file, page, location, excerpt) to every result. Tauri v2 + Rust + React/TS. Hush is free and open source. Every feature works the same for every user, and sponsorships pay for maintenance. Supporter is a self-reported setting that only unlocks a thank-you badge and theme.

## Absolute rules (if a task would require breaking one, stop and report)

1. Do not write code that sends document content, chunks, embeddings, queries, or file paths over the network.
2. All outbound HTTP goes through the single `net::Client`, and every request first calls `security::airgap::AirGapGuard::check(NetPurpose)`. Only two purposes are allowed: ModelDownload and UpdateCheck, each with exactly one allowed host. There is no telemetry. Sponsorship is handled only through the external browser or a QR code; the app makes no requests for it.
3. File reads are allowed only on paths validated by `security::paths::ScopedPath`. Paths outside registered folders, `..`, and symlink escapes are forbidden.
4. Paths, snippets, metadata, vectors, the FTS index, and change history are stored only in the single SQLCipher DB. Plaintext caches and temp files are forbidden.
5. Logs never contain document bodies, snippets, queries, file names, or keys. Use doc_id when identification is needed. Counters such as search counts may be logged, but query strings are never stored anywhere.
6. Keys and tokens are handled only as `Zeroizing`/`SecretBox` and never implement Debug/Display.
7. Sentences in an LLM answer that lack a citation are never shown to the user. When evidence is insufficient, return 「제공된 로컬 문서에서 확인되지 않음」 ("Not found in the provided local documents").
8. Every feature works the same for every user. No feature locks, usage limits, upgrade prompts, payment code, or license checks. Sponsoring changes nothing but symbolic thanks (README list with consent, in-app badge and theme, roadmap votes).
9. Brand name, bundle ID, keychain service name, file extension, and domain are referenced only through constants in `brand.rs` (Rust) and `src/brand.ts` (generated at build time). Hardcoding these strings is forbidden.
10. The public repository is the complete source of every official build. No private modules, obfuscation, or build-time code that is not in the repository.

## Module boundaries

- ingest/: file → ParsedDocument → Vec<Chunk>. Knows nothing about the DB or models, except `ingest/pipeline.rs`, which calls `Db` methods from `storage/documents.rs` (all SQL stays in storage).
- rag/: embedding, hybrid (vector + keyword + reranker), synthesizer (cited Q&A and guards), hushpack. Knows nothing about the file system.
- storage/: SQLCipher connection, schema and migrations, FTS, change history, keychain.
- security/: AirGapGuard, ScopedPath, pii_guard.
- commands.rs: IPC entry points only. Keep the order: service call → AppError conversion.
- Each module has its own error.rs (thiserror). Convert to AppError { code, message } only at the IPC boundary.

## Coding rules

- Rust stable, edition 2021. unwrap/expect only in tests. unsafe only for FFI, with a SAFETY comment.
- Parsing, OCR, embedding, reranking, and LLM work run in spawn_blocking or a dedicated worker. Never block the async runtime.
- Before using a new crate, record its reason, license, and network use in docs/decisions.md. Disable or replace any runtime auto-download feature.
- IPC types are generated from Rust (tauri-specta or ts-rs). Use TS strict, and never define a type by hand twice.
- DB schema changes always go in a new migration. Never modify a migration that has shipped.

## Open source governance

- The project is GPL-3.0-or-later (`LICENSE`). `src-tauri/Cargo.toml` and `package.json` declare `GPL-3.0-or-later`. Never change the license, `GOVERNANCE.md`, or an absolute rule without an issue approved by the lead maintainer.
- Every dependency must be GPL-3.0-compatible (Apache-2.0, MIT, BSD, MPL-2.0 are fine; GPL-2.0-only, AGPL, and non-commercial licenses are not). `cargo deny check licenses` (`src-tauri/deny.toml`) enforces this for crates. Widening its allowlist needs an ADR in docs/decisions.md. Model weights are data, not program code; only models whose license allows redistribution go in the offline kit. Never copy third-party code without checking its license and keeping its notice.
- Commits use Conventional Commits with a scope (`feat(storage): ...`) and a DCO sign-off (`git commit -s`). There is no CLA.
- Distribution is GitHub Releases and Homebrew only (the Mac App Store terms conflict with the GPL). Every release uploads the source archive of its tag, and the About screen shows the license notice and a source link.
- Forks use a different name and icon (`TRADEMARK.md`). Sponsorship runs through GitHub Sponsors and Buy Me a Coffee (`.github/FUNDING.yml`); totals and spending are published quarterly in `docs/funding.md`, with no sponsor personal data.
- Keep `GOVERNANCE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, `TRADEMARK.md`, `CODE_OF_CONDUCT.md`, and `.github/` (CODEOWNERS, issue forms, PR template, FUNDING.yml) in sync with these rules. A change to the absolute rules, the checks, or the maintainers updates them in the same change.
- Issues, PRs, fixtures, and examples never contain real document content, queries, file paths, or keys. Vulnerabilities go through GitHub private vulnerability reporting, never public issues.
- Do not add terms that restrict GPL rights.

## How to work

- Before implementing, present the input/output structs, error types, and public signatures.
- Feature code and its unit tests go in the same change.
- Before finishing, run: cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && pnpm lint && pnpm typecheck && pnpm test
- When done, append the date, what was done, TODOs, and constraints to docs/progress.md.
- Work follows the 12-week plan in `docs/roadmap.md`: one prompt per fresh session, commit when its DoD passes, a security review at the end of each week, and a PR review for every external PR. When a week runs late, drop items from its cut line first.
- If a requirement is ambiguous or conflicts with an absolute rule, present 2–3 options and stop.
- The project language is English: code, comments, docs, commit messages, and PRs.
