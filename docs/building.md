# Building Locite yourself

Official builds are signed and notarized, and published on GitHub Releases and Homebrew. You can also build and run the same code yourself. The source in this repository is the complete source of the official builds.

## Requirements

- macOS on Apple Silicon (the keychain integration is macOS-only for now)
- [Rust](https://rustup.rs/) stable
- Node.js 26 and [pnpm](https://pnpm.io/)

## Build

```sh
git clone https://github.com/skyoo2003/hush.git
cd locite
pnpm install
bash scripts/fetch-pdfium.sh   # once; pinned PDFium, SHA-256 checked
pnpm tauri build
```

The app bundle is written to `src-tauri/target/release/bundle/macos/`. For a development build with reload, run `pnpm tauri dev` instead.

To build a specific release, check out its tag first (`git checkout vX.Y.Z`), or download the source archive attached to that release.

## Notes

- `scripts/fetch-pdfium.sh` downloads the PDF engine at build time. The app itself never downloads it.
- Self-built copies are unsigned, so macOS Gatekeeper blocks the first launch. Open it with right-click, then **Open**.
- Unsigned builds may ask for keychain access again after each rebuild. [CONTRIBUTING.md](../CONTRIBUTING.md) shows how to sign dev builds with your own identity.
- If you distribute your build to others, use a different name and icon. See [TRADEMARK.md](../TRADEMARK.md).
