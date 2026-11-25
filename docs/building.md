# Building Locite yourself

Official builds are signed with the project's self-signed release certificate (not notarized, [ADR-021](decisions.md)) and published on GitHub Releases and Homebrew by [`.github/workflows/release.yml`](../.github/workflows/release.yml). Every release also carries the source archive it was built from. This repository is the complete source of every official build, and the GPL gives you the right to build, change, and run it yourself.

## Requirements

- macOS on Apple Silicon, with the Xcode Command Line Tools (`xcode-select --install`)
- [rustup](https://rustup.rs/). It installs the Rust version pinned in `rust-toolchain.toml` (1.98.1, with rustfmt, clippy, and the `aarch64-apple-darwin` target) on first use.
- Node.js 26
- pnpm at the version in `package.json`'s `packageManager` field (10.27.0); `corepack enable` picks it up.

## Build

From a release's source archive (`locite-X.Y.Z-source.tar.gz`, after checking it as below):

```sh
tar xzf locite-X.Y.Z-source.tar.gz && cd locite-X.Y.Z
```

or from a tag:

```sh
git clone https://github.com/skyoo2003/locite.git && cd locite
git checkout vX.Y.Z
```

Then:

```sh
pnpm install --frozen-lockfile
bash scripts/fetch-pdfium.sh   # pinned PDFium, SHA-256 checked; the app itself never downloads it
pnpm tauri build --bundles app --target aarch64-apple-darwin
bash scripts/sign-macos.sh src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Locite.app
bash scripts/check-signatures.sh src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Locite.app
```

`sign-macos.sh` ad-hoc signs every library and then the app, the same steps the release workflow runs with its certificate; add an output folder (`sign-macos.sh <App.app> out`) to also get the DMG and the updater archive. For a development build with reload, run `pnpm tauri dev` instead.

Gatekeeper does not trust an ad-hoc signature. A build you made on this Mac carries no quarantine flag and opens normally. If you copied it from somewhere else, open **System Settings > Privacy & Security** after the first blocked launch and click **Open Anyway**, or clear the flag with `xattr -dr com.apple.quarantine Locite.app`. Do this only for a build you made yourself or an official one you checked as below.

An ad-hoc signature changes on every build, so the login keychain asks for your password again to release the database key. [CONTRIBUTING.md](../CONTRIBUTING.md) shows how to sign with your own identity instead.

## Running a modified version

You may change the source and run or share your version under GPL-3.0-or-later. If you distribute it, give it a different name and icon (change `src-tauri/src/brand.rs` and the icons), as [TRADEMARK.md](../TRADEMARK.md) asks.

Your build keeps the official updater key in `plugins.updater.pubkey` (`src-tauri/tauri.conf.json`), so **Check for updates** would replace your build with the next official one. Set the key to `""` to turn updates off (the check then fails before any request), or put your own key from `pnpm tauri signer generate` there and serve your own `latest.json`.

## Checking an official build

Verify the files of a release against their checksums and the build provenance GitHub recorded for them:

```sh
gh release download vX.Y.Z --repo skyoo2003/locite
shasum -a 256 -c SHA256SUMS
gh attestation verify Locite_X.Y.Z_aarch64.dmg --repo skyoo2003/locite
gh attestation verify locite-X.Y.Z-source.tar.gz --repo skyoo2003/locite
```

The attestation proves the file was produced by `release.yml` in this repository from that tag. An installed official app is signed by the committed release certificate, [`packaging/macos/release-signing-cert.pem`](../packaging/macos/release-signing-cert.pem):

```sh
openssl x509 -in packaging/macos/release-signing-cert.pem -noout -fingerprint -sha1   # AE:5B:09:...:0F:AE
codesign -d -r- /Applications/Locite.app   # certificate leaf = H"ae5b094c9db1bafb6cb4140bb29d9d98025d0fae"
```

To compare an official app with your own build of the same tag, copy both, strip the signatures, and diff:

```sh
codesign --remove-signature Official.app/Contents/MacOS/locite Official.app/Contents/Frameworks/libpdfium.dylib
codesign --remove-signature Mine.app/Contents/MacOS/locite Mine.app/Contents/Frameworks/libpdfium.dylib
diff -r -x _CodeSignature -x CodeResources Official.app Mine.app   # signature files differ by signer
```

Builds are not bit-identical yet. Building one commit twice on one Mac in two directories (measured 2026-11-25) gave bundles that matched byte for byte after the signatures were removed, except `Contents/MacOS/locite`: the vendored OpenSSL under SQLCipher embeds its build time and the absolute build directory, and the Mach-O UUID changes with them. So expect `diff -r` to report only that executable. Its link to the source is the tag and the attestation, not a byte comparison.
