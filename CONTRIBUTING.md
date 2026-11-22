# Contributing

Thanks for helping. Hush indexes private documents entirely on-device, so every change is held to the privacy rules below before anything else.

## Before you start

- Read the absolute rules in [CLAUDE.md](CLAUDE.md). A pull request that breaks one is closed, however useful it is.
- For anything larger than a bug fix, open an issue first so the design can be agreed before you write code.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Development setup

You need Rust stable, Node.js 26, and pnpm. The crate currently builds only on macOS because of the keychain key provider.

```sh
pnpm install
bash scripts/fetch-pdfium.sh   # once; pinned PDFium, SHA-256 checked
pnpm tauri dev
```

PDF parsing loads a native PDFium library. `scripts/fetch-pdfium.sh` downloads the pinned build into `src-tauri/vendor/pdfium/` (git-ignored) and skips the download when it is already there. The build bundles that library, so `cargo build` fails until the script has run, and the PDF tests panic with "run scripts/fetch-pdfium.sh". The app itself never downloads it.

Dev builds are ad-hoc signed, so every rebuild looks like a new app to the login keychain, and macOS asks for your password again to release the DB key. To stop that, sign dev builds with a code-signing identity you already have (an Apple Development certificate works):

```sh
security find-identity -v -p codesigning           # pick a SHA-1 or name
export HUSH_DEV_SIGN_IDENTITY=<SHA-1 or name>      # e.g. in ~/.zshrc
```

`cargo run` and `pnpm tauri dev` start the app through `scripts/dev-sign.sh`, which re-signs it with that identity and a fixed identifier. Click "Always Allow" on the next prompt and it will not come back after rebuilds. With the variable unset, nothing changes.

## Checks

Run everything CI runs before you push:

```sh
pnpm lint && pnpm typecheck && pnpm test && pnpm build
bash scripts/check-brand.sh
cd src-tauri
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo deny check licenses
```

## Pull requests

- Keep one logical change per pull request, with its tests in the same change. A tier-gated feature includes tests that it is rejected on Free and allowed on the permitted tiers.
- Write commit messages in [Conventional Commits](https://www.conventionalcommits.org/) form with a scope, for example `feat(storage): ...` or `fix(rag): ...`.
- Write code, comments, docs, and commit messages in English.
- Add an entry to [docs/progress.md](docs/progress.md), and an ADR to [docs/decisions.md](docs/decisions.md) for any architecture decision.
- Never paste real document contents, queries, file paths, or keys into issues, pull requests, logs, or test fixtures.

## New dependencies

Record the reason, the license, and any network use in [docs/decisions.md](docs/decisions.md) before adding a crate or npm package. The license must be compatible with GPL-3.0-only. If `cargo deny check licenses` rejects it, do not widen `src-tauri/deny.toml` without an ADR explaining why the license is compatible.

## Developer Certificate of Origin

All commits must be signed off. By signing off you certify the [Developer Certificate of Origin 1.1](https://developercertificate.org/), which says you wrote the change or have the right to submit it under the project license.

```sh
git commit -s
```

This adds a `Signed-off-by: Your Name <you@example.com>` line that must match the commit author.

## License

By contributing, you agree that your contributions are licensed under [GPL-3.0-only](LICENSE).
