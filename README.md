# Locite

**Ask your own documents questions, without anything leaving your computer.**

Locite is a desktop app that indexes, searches, and summarizes the documents in folders you choose. Everything runs on your device. Every answer shows where it came from: the file, the page, the location, and the exact excerpt.

> **Status: pre-alpha.** Encrypted storage, the Air-Gap guard, document ingestion, the folder watcher, and hybrid search work in the backend. Q&A is not built yet, so the app is not useful to end users today. Development targets macOS on Apple Silicon only. Expect breaking changes.

<!-- Screenshots: add once the quick-search UI ships (roadmap week 5).
![Quick search](docs/images/quick-search.png)
![Answer with citations](docs/images/answer-citations.png)
-->

## Why Locite

Most "chat with your documents" tools upload your files to someone else's server. Locite is built on the opposite assumption. Your documents, the text pulled out of them, and the questions you ask stay on your machine.

- **Zero cloud.** Parsing, embedding, search, and answer generation all run locally. The app never sends document content, chunks, embeddings, queries, or file paths over the network.
- **Answers with receipts.** Locite only shows a sentence if it can cite a source in your documents. When the evidence isn't there, it says so rather than guessing.
- **Encrypted at rest.** The index lives in a single SQLCipher-encrypted database. The key is kept in the macOS Keychain. There are no plaintext caches or temp files.
- **You pick the folders.** Locite only reads inside folders you register. It rejects `..` paths and symlinks that point outside them.
- **Private logs.** Logs never contain document text, snippets, queries, file names, or keys.

## The Air-Gap principle

The app makes outbound network requests in exactly two cases:

1. **Model download.** Fetching a model you asked for, from `huggingface.co`.
2. **Update check.** Checking for a new app version on GitHub Releases, only when you click "Check for updates". An update installs only if its signature matches the key built into the app.

There is no telemetry. Each starts at one fixed host (`huggingface.co` or `github.com`), and may then follow a redirect only to that service's download servers (`*.hf.co`, or `release-assets.githubusercontent.com`). Every downloaded model file is checked against a SHA-256 pinned in the app, and fully offline machines can import the files instead. All HTTP goes through a single client, and an **Air-Gap guard** checks every request before it is sent. Air-Gap mode is on by default.

The code in this repository is all of the code in official builds. There are no private modules, so anyone can check the claim that nothing leaves the device.

## Sponsors

Locite is free. Every feature works the same for everyone, with no feature locks, usage limits, or upgrade prompts. The app contains no payment or license code.

Maintenance is paid for by sponsors through [GitHub Sponsors](https://github.com/sponsors/skyoo2003) (monthly) and [Buy Me a Coffee](https://buymeacoffee.com/skyoo2003) (one-time). Sponsoring gets you no extra features. Thanks are symbolic: a place in the thank-you list below if you agree to it, an in-app thank-you badge you turn on yourself, and votes on the roadmap. The badge's OLED dark and Terminal themes are open to everyone. See [docs/funding.md](docs/funding.md) for the funding goals, the quarterly totals and spending, and how the thank-you list works.

### Thank you

No sponsors yet.

## Install

There is no release yet. Build from source as shown below. Once releases start, Locite is distributed through GitHub Releases and Homebrew only:

```sh
brew install --cask skyoo2003/tap/locite
```

It is not in the Mac App Store, whose terms conflict with the GPL. Each release also carries the source archive of its tag. See [SUPPORT.md](SUPPORT.md) for supported macOS versions.

Releases are signed with the project's own certificate and not notarized yet: notarization needs the paid Apple Developer Program, the first [sponsorship goal](docs/funding.md). The Homebrew cask handles this for you. If you open the DMG yourself, macOS blocks the first launch; open **System Settings > Privacy & Security** and click **Open Anyway**. Check the DMG against the release's `SHA256SUMS` and attestation first ([docs/building.md](docs/building.md#checking-an-official-build)).

## Roadmap

Work follows a 12-week plan to v1.0 in [docs/roadmap.md](docs/roadmap.md), and [CHANGELOG.md](CHANGELOG.md) records what has landed. Planned features:

- Index PDFs and other documents from folders you choose, and keep the index up to date as files change.
- Hybrid search that combines vector similarity, keyword search, and a reranker.
- Source-grounded Q&A and summaries from a local LLM.
- A quick-search window on a global shortcut (`Cmd+Shift+Space`, changeable in settings) and a menu bar icon; no Dock icon unless the main window is open.
- `.locitepack` files for packaging an index.

## Building from source

You need a Mac with Apple Silicon. The keychain integration is macOS-only for now.

1. Install the Xcode Command Line Tools, Rust stable, Node.js 26, and pnpm 10:

   ```sh
   xcode-select --install
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   brew install node pnpm   # or any other way to get Node.js 26 and pnpm 10
   ```

2. Clone, install, and fetch the pinned PDF engine (SHA-256 checked, git-ignored):

   ```sh
   git clone https://github.com/skyoo2003/locite.git
   cd locite
   pnpm install
   bash scripts/fetch-pdfium.sh
   ```

3. Run the tests. `pnpm build` comes first because the Rust crate embeds the built frontend.

   ```sh
   pnpm lint && pnpm typecheck && pnpm test && pnpm build
   cd src-tauri && cargo test
   ```

4. Run the app with `pnpm tauri dev`, or build a bundle with `pnpm tauri build`.

On first launch the app creates an encrypted database under `~/Library/Application Support/` and stores its key in the Keychain. Unsigned dev builds may ask for Keychain access again after a rebuild. [CONTRIBUTING.md](CONTRIBUTING.md) shows how to avoid that, and [docs/building.md](docs/building.md) covers release builds. Tests that need real model weights are `#[ignore]`d. Run `bash scripts/fetch-models.sh` and then `cargo test --release -- --ignored` to include them.

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
| `docs/architecture.md` | Module structure, data flow, and the network boundary. |
| `src-tauri/src/commands.rs` | IPC entry points called by the frontend. |
| `src/` | React frontend. |
| `docs/decisions.md` | Architecture decision records, with benchmarks. |
| `docs/roadmap.md` | The 12-week plan to v1.0 and beyond. |

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the checks CI runs, and the DCO sign-off. The privacy rules in [CLAUDE.md](CLAUDE.md) apply to every change, whoever writes it. [GOVERNANCE.md](GOVERNANCE.md) explains how decisions are made, and everyone is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

Found a way for data to leave the device? Please report it privately as described in [SECURITY.md](SECURITY.md), not in a public issue.

## License

Locite is licensed under [GPL-3.0-or-later](LICENSE). You may fork, modify, and redistribute it, even commercially, but anything you distribute must ship with its complete source under the same license. Forks distributed to others must also use a different name and icon, as described in [TRADEMARK.md](TRADEMARK.md).

Model weights are separate data files, not part of the program, and each model's license is recorded in the model catalog. Only models whose license allows redistribution are included in the offline kit.
