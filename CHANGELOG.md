# Changelog

All notable changes to Locite are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Offline install: each release carries `models-kit-X.Y.Z.zip` with the redistributable embedding models, their licenses, a manifest, and `SHA256SUMS`. **Import a model file or kit…** installs the whole kit after checking every file against the catalog's SHA-256 ([docs/offline-install.md](docs/offline-install.md)).

## [0.1.0] - 2026-11-25

### Added

- Encrypted storage: one SQLCipher database with FTS5 and sqlite-vec, keyed from the macOS Keychain.
- Air-Gap guard: all HTTP goes through one client, limited to model download and update check, each starting at one host (`huggingface.co`, `github.com`) and redirected only to that service's download host.
- Signed manual updates (Security panel): "Check for updates" reads the latest GitHub release's `latest.json`, and an update installs only if its minisign signature matches the app's key and the version it was signed for. Nothing checks on its own.
- Scoped file access: reads only inside registered folders, with `..` and symlink escapes rejected.
- Document ingestion: PDF and image parsing, OCR through macOS Vision, chunking, and SHA-256 change detection.
- Local embeddings (bge-small-en-v1.5 by default) and hybrid search (vector + keyword, fused by RRF).
- Folder scanner, index queue, and folder watcher with incremental reindexing.
- Index scheduler: indexing yields to search, slows on low battery, Low Power Mode, or heat, speeds up when you are away, and follows Pause, Run now, and allowed hours (Folders panel and tray).
- Model manager: a built-in catalog with pinned SHA-256s, resumable downloads that Air-Gap refuses or stops, and manual import for offline machines (Security panel).
- Release pipeline: tagged releases are built without secrets, then signed with the project's release certificate after maintainer approval (not notarized yet), and published as drafts with the source archive, an SBOM, third-party notices, SHA256SUMS, and build provenance attestations. Install with `brew install --cask skyoo2003/tap/locite`.
- Repository governance: GPL-3.0-or-later license, contribution guide with DCO, security policy, trademark policy, and CI checks for licenses, advisories, SPDX headers, DCO sign-off, and secrets.

[Unreleased]: https://github.com/skyoo2003/locite/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/skyoo2003/locite/releases/tag/v0.1.0
