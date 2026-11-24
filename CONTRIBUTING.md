# Contributing

Thanks for helping. Locite indexes private documents entirely on-device, so every change is held to the privacy rules below before anything else.

## Before you start

- Read the absolute rules in [CLAUDE.md](CLAUDE.md). A pull request that breaks one is closed, however useful it is.
- For anything larger than a bug fix, open an issue first so the design can be agreed before you write code.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- Locite is free for everyone. Pull requests that add feature locks, usage limits, payment or license code, or telemetry are closed.
- Maintainers triage once a week, so expect a reply within about a week.

## Development setup

You need macOS on Apple Silicon, the Xcode Command Line Tools (`xcode-select --install`), [Rust](https://rustup.rs/) stable, Node.js 26, and pnpm 10. The crate builds only on macOS because of the keychain key provider and Vision OCR.

```sh
pnpm install
bash scripts/fetch-pdfium.sh   # once; pinned PDFium, SHA-256 checked
pnpm tauri dev
```

PDF parsing loads a native PDFium library. `scripts/fetch-pdfium.sh` downloads the pinned build into `src-tauri/vendor/pdfium/` (git-ignored) and skips the download when it is already there. The build bundles that library, so `cargo build` fails until the script has run, and the PDF tests panic with "run scripts/fetch-pdfium.sh". The app itself never downloads it.

Dev builds are ad-hoc signed, so every rebuild looks like a new app to the login keychain, and macOS asks for your password again to release the DB key. To stop that, sign dev builds with a code-signing identity you already have (an Apple Development certificate works):

```sh
security find-identity -v -p codesigning           # pick a SHA-1 or name
export LOCITE_DEV_SIGN_IDENTITY=<SHA-1 or name>      # e.g. in ~/.zshrc
```

`cargo run` and `pnpm tauri dev` start the app through `scripts/dev-sign.sh`, which re-signs it with that identity and a fixed identifier. Click "Always Allow" on the next prompt and it will not come back after rebuilds. With the variable unset, nothing changes.

## Running tests and checks

Run everything CI runs before you push. `pnpm build` comes before `cargo test` because the crate embeds the built frontend.

```sh
pnpm lint && pnpm typecheck && pnpm test && pnpm build
bash scripts/check-brand.sh        # brand strings only via brand.rs
bash scripts/check-spdx.sh         # SPDX header on every source file (--fix adds it)
node scripts/check-npm-licenses.mjs
bash scripts/check-net-usage.sh    # networking APIs only in src-tauri/src/net; rewrites docs/network-deps.md
node scripts/check-dist-urls.mjs   # no external URLs in dist/ outside its allow list
cd src-tauri
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo deny check                   # licenses, advisories, bans, sources
cargo audit                        # advisories for every Cargo.lock entry (cargo install cargo-audit)
```

Tests that need real model weights are `#[ignore]`d. Run `bash scripts/fetch-models.sh`, then `cargo test --release -- --ignored`.

CI also checks DCO sign-off on every pull request commit (`scripts/check-dco.sh`) and scans the full history for secrets with [gitleaks](https://github.com/gitleaks/gitleaks).

Every new source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later` (`#` in shell scripts, after the shebang).

## Bug reports

Never paste real document content into bug reports. The same goes for queries, file names, paths, and keys. Reproduce the bug with a small sample file you made yourself, and attach that instead. Use the issue form, which asks for your macOS version and chip.

## Pull requests

1. For anything bigger than a bug fix, agree on the design in an issue first.
2. Fork the repository and branch from `main`.
3. Make the change with its tests, and run the checks above.
4. Open a pull request and fill in the template. Say whether it touches network code or adds a dependency.
5. A maintainer reviews it against the absolute rules in [CLAUDE.md](CLAUDE.md). One maintainer approval and green CI are enough to merge. Pull requests are squash-merged.

Workflows run on `pull_request` only, never `pull_request_target`, so pull requests from forks get a read-only token and no secrets.

Guidelines:

- Keep one logical change per pull request, with its tests in the same change.
- Write commit messages in [Conventional Commits](https://www.conventionalcommits.org/) form with a scope, for example `feat(storage): ...` or `fix(rag): ...`.
- Write code, comments, docs, and commit messages in English.
- Add an ADR to [docs/decisions.md](docs/decisions.md) for any architecture decision.
- Never paste real document contents, queries, file paths, or keys into issues, pull requests, logs, or test fixtures.

## New dependencies

Record the reason, the license, and any network use in [docs/decisions.md](docs/decisions.md) before adding a crate or npm package. The license must be compatible with GPL-3.0 (Apache-2.0, MIT, BSD, and MPL-2.0 are; GPL-2.0-only, AGPL, and non-commercial licenses are not). If `cargo deny check licenses` rejects it, do not widen `src-tauri/deny.toml` without an ADR explaining why the license is compatible.

## Developer Certificate of Origin

All commits must be signed off. By signing off you certify the [Developer Certificate of Origin 1.1](https://developercertificate.org/), which says you wrote the change or have the right to submit it under the project license.

```sh
git commit -s
```

This adds a `Signed-off-by: Your Name <you@example.com>` line that must match the commit author.

## License

By contributing, you agree that your contributions are licensed under [GPL-3.0-or-later](LICENSE). Copyright stays with you; there is no CLA.
