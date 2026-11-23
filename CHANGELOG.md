# Changelog

All notable changes to Locite are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Encrypted storage: one SQLCipher database with FTS5 and sqlite-vec, keyed from the macOS Keychain.
- Air-Gap guard: all HTTP goes through one client, limited to model download and update check, each to one host.
- Scoped file access: reads only inside registered folders, with `..` and symlink escapes rejected.
- Document ingestion: PDF and image parsing, OCR through macOS Vision, chunking, and SHA-256 change detection.
- Local embeddings (bge-small-en-v1.5 by default) and hybrid search (vector + keyword, fused by RRF).
- Folder scanner, index queue, and folder watcher with incremental reindexing.
- Repository governance: GPL-3.0-or-later license, contribution guide with DCO, security policy, trademark policy, and CI checks for licenses, advisories, SPDX headers, DCO sign-off, and secrets.
