# Hush

**Ask your own documents questions, without anything leaving your computer.**

Hush is a desktop app that indexes, searches, and summarizes the documents in folders you choose. Everything runs on your device. Every answer shows where it came from: the file, the page, the location, and the exact excerpt.

> **Status: pre-alpha.** The storage, encryption, and network-guard foundations are in place. Document ingestion, search, and Q&A are not implemented yet, so the app does not do anything useful for end users today. Development currently targets macOS on Apple Silicon only.

## Why Hush

Most "chat with your documents" tools upload your files to someone else's server. Hush is built on the opposite assumption. Your documents, the text pulled out of them, and the questions you ask stay on your machine.

- **Zero cloud.** Parsing, embedding, search, and answer generation all run locally. The app never sends document content, chunks, embeddings, queries, or file paths over the network.
- **Answers with receipts.** Hush only shows a sentence if it can cite a source in your documents. When the evidence isn't there, it says so rather than guessing.
- **Encrypted at rest.** The index lives in a single SQLCipher-encrypted database. The key is kept in the macOS Keychain. There are no plaintext caches or temp files.
- **You pick the folders.** Hush only reads inside folders you register. It rejects `..` paths and symlinks that point outside them.
- **Private logs.** Logs never contain document text, snippets, queries, file names, or keys.

## How it stays offline

The app makes outbound network requests for exactly three reasons: verifying a license, downloading a model you asked for, and checking for app updates. Each is allowed to reach one fixed host. All HTTP goes through a single client, and an **Air-Gap guard** checks every request before it is sent. Air-Gap mode is on by default.

## Free and paid tiers

Hush comes in Free, Lite, and Pro tiers. Paid tiers only add features. They never limit how many documents or folders you can index, never lower search quality, and never turn off safety features. If a subscription ends, your data is never deleted or hidden.

The code behind paid tiers is GPL source like the rest of the project.

## Planned features

- Index PDFs and other documents from folders you choose, and keep the index up to date as files change.
- Hybrid search that combines vector similarity, keyword search, and a reranker.
- Source-grounded Q&A and summaries from a local LLM.
- A quick-search window on a global shortcut (`Cmd+Shift+Space`).
- `.hushpack` files for packaging an index.

## Building from source

Requirements:

- macOS (the keychain integration is macOS-only for now)
- [Rust](https://rustup.rs/) stable
- Node.js 26 and [pnpm](https://pnpm.io/)

```sh
git clone https://github.com/skyoo2003/hush.git
cd hush
pnpm install
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
| `src-tauri/src/rag/` | Embeddings, hybrid search, answer synthesis, hushpack. |
| `src-tauri/src/storage/` | Encrypted database, schema migrations, full-text index, keychain. |
| `src-tauri/src/security/` | Air-Gap guard, scoped path validation, PII guard, license verification. |
| `src-tauri/src/entitlement.rs` | Tier permission table. |
| `src-tauri/src/net.rs` | The only HTTP client in the app. |
| `src-tauri/src/commands.rs` | IPC entry points called by the frontend. |
| `src/` | React frontend. |
| `docs/decisions.md` | Architecture decision records, with benchmarks. |
| `docs/progress.md` | Running log of what's done and what's next. |

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the checks CI runs, and the DCO sign-off. The privacy rules in [CLAUDE.md](CLAUDE.md) apply to every change, whoever writes it. [GOVERNANCE.md](GOVERNANCE.md) explains how decisions are made, and everyone is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

Found a way for data to leave the device? Please report it privately as described in [SECURITY.md](SECURITY.md), not in a public issue.

## License

Hush is licensed under [GPL-3.0-only](LICENSE).

Model weights are separate data files, not part of the program, and each model's license is recorded in the model catalog.
