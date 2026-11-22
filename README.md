# Locite

**Ask your own documents questions, without anything leaving your computer.**

Locite is a desktop app that indexes, searches, and summarizes the documents in folders you choose. Everything runs on your device. Every answer shows where it came from: the file, the page, the location, and the exact excerpt.

> **Status: pre-alpha.** The storage, encryption, and network-guard foundations are in place. Document ingestion, search, and Q&A are not implemented yet, so the app does not do anything useful for end users today. Development currently targets macOS on Apple Silicon only.

## Why Locite

Most "chat with your documents" tools upload your files to someone else's server. Locite is built on the opposite assumption. Your documents, the text pulled out of them, and the questions you ask stay on your machine.

- **Zero cloud.** Parsing, embedding, search, and answer generation all run locally. The app never sends document content, chunks, embeddings, queries, or file paths over the network.
- **Answers with receipts.** Locite only shows a sentence if it can cite a source in your documents. When the evidence isn't there, it says so rather than guessing.
- **Encrypted at rest.** The index lives in a single SQLCipher-encrypted database. The key is kept in the macOS Keychain. There are no plaintext caches or temp files.
- **You pick the folders.** Locite only reads inside folders you register. It rejects `..` paths and symlinks that point outside them.
- **Private logs.** Logs never contain document text, snippets, queries, file names, or keys.

## How it stays offline

The app makes outbound network requests for exactly two reasons: downloading a model you asked for, and checking for app updates. There is no telemetry. Each is allowed to reach one fixed host. All HTTP goes through a single client, and an **Air-Gap guard** checks every request before it is sent. Air-Gap mode is on by default.

The code in this repository is all of the code in official builds. There are no private modules, so anyone can check the claim that nothing leaves the device.

## Free, funded by sponsors

Locite is free. Every feature works the same for everyone, with no feature locks, usage limits, or upgrade prompts. The app contains no payment or license code.

Maintenance is paid for by sponsors through [GitHub Sponsors](https://github.com/sponsors/skyoo2003) (monthly) and [Buy Me a Coffee](https://buymeacoffee.com/skyoo2003) (one-time). Sponsoring gets you no extra features. Thanks are symbolic: a place in the README thank-you list if you want it, an in-app badge and theme you turn on yourself, and votes on the roadmap. Funding goals, totals, and spending are published every quarter in [docs/funding.md](docs/funding.md).

## Install

Locite is distributed through GitHub Releases and Homebrew only. It is not in the Mac App Store, whose terms conflict with the GPL. Each release also carries the source archive of its tag. See [SUPPORT.md](SUPPORT.md) for supported macOS versions.

## Planned features

- Index PDFs and other documents from folders you choose, and keep the index up to date as files change.
- Hybrid search that combines vector similarity, keyword search, and a reranker.
- Source-grounded Q&A and summaries from a local LLM.
- A quick-search window on a global shortcut (`Cmd+Shift+Space`).
- `.locitepack` files for packaging an index.

## Building from source

The full guide is [docs/building.md](docs/building.md). The short version:

Requirements:

- macOS (the keychain integration is macOS-only for now)
- [Rust](https://rustup.rs/) stable
- Node.js 26 and [pnpm](https://pnpm.io/)

```sh
git clone https://github.com/skyoo2003/locite.git
cd locite
pnpm install
bash scripts/fetch-pdfium.sh
pnpm tauri dev
```

On first launch the app creates an encrypted database under `~/Library/Application Support/` and stores its key in the Keychain. Unsigned dev builds may ask for Keychain access again after a rebuild.

## Tech stack

- [Tauri v2](https://v2.tauri.app/) with a Rust backend and a React + TypeScript frontend (Vite)
- [SQLCipher](https://www.zetetic.net/sqlcipher/) with [sqlite-vec](https://github.com/asg017/sqlite-vec) and FTS5, all in one encrypted database
- [rustls](https://github.com/rustls/rustls) for the few allowed network requests

## Project layout

| Path | What lives there |
| --- | --- |
| `src-tauri/src/ingest/` | Turns files into parsed documents and chunks. |
| `src-tauri/src/rag/` | Embeddings, hybrid search, answer synthesis, locitepack. |
| `src-tauri/src/storage/` | Encrypted database, schema migrations, full-text index, keychain. |
| `src-tauri/src/security/` | Air-Gap guard, scoped path validation, PII guard. |
| `src-tauri/src/net.rs` | The only HTTP client in the app. |
| `src-tauri/src/commands.rs` | IPC entry points called by the frontend. |
| `src/` | React frontend. |
| `docs/decisions.md` | Architecture decision records, with benchmarks. |
| `docs/roadmap.md` | The 12-week plan to v1.0 and beyond. |

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the checks CI runs, and the DCO sign-off. The privacy rules in [CLAUDE.md](CLAUDE.md) apply to every change, whoever writes it. [GOVERNANCE.md](GOVERNANCE.md) explains how decisions are made, and everyone is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

Found a way for data to leave the device? Please report it privately as described in [SECURITY.md](SECURITY.md), not in a public issue.

## License

Locite is licensed under [GPL-3.0-or-later](LICENSE). Forks are welcome under that license, but must use a different name and icon. See [TRADEMARK.md](TRADEMARK.md).

Model weights are separate data files, not part of the program, and each model's license is recorded in the model catalog. Only models whose license allows redistribution are included in the offline kit.
