# Changelog

All notable changes to Locite are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Encrypted storage: one SQLCipher database with FTS5 and sqlite-vec, keyed from the macOS Keychain.
- Air-Gap guard: all HTTP goes through one client, limited to model download and update check, each starting at one host.
- Scoped file access: reads only inside registered folders, with `..` and symlink escapes rejected.
- Document ingestion: PDF and image parsing, OCR through macOS Vision, chunking, and SHA-256 change detection.
- Local embeddings (bge-small-en-v1.5 by default) and hybrid search (vector + keyword, fused by RRF).
- Folder scanner, index queue, and folder watcher with incremental reindexing.
- Index scheduler: indexing yields to search, slows on low battery, Low Power Mode, or heat, speeds up when you are away, and follows Pause, Run now, and allowed hours (Folders panel and tray).
- Model manager: a built-in catalog with pinned SHA-256s, resumable downloads that Air-Gap refuses or stops, and manual import for offline machines (Security panel).
- Release pipeline: tagged releases are built without secrets, then signed, notarized, and stapled after maintainer approval, and published as drafts with the source archive, an SBOM, third-party notices, SHA256SUMS, and build provenance attestations. A draft Homebrew cask is in `packaging/homebrew/`.
- Repository governance: GPL-3.0-or-later license, contribution guide with DCO, security policy, trademark policy, and CI checks for licenses, advisories, SPDX headers, DCO sign-off, and secrets.
