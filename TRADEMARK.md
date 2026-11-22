# Trademark policy

The code is GPL-3.0-or-later, and you may fork, modify, and redistribute it under that license, including selling it, as long as you publish your source. The license covers the code. It does not cover the project's name and icon.

## Rules

- A fork that is distributed to others must use a different name and a different icon. Change the constants in `src-tauri/src/brand.rs` and replace `src-tauri/icons/`.
- A fork must not suggest that it is the official Hush app or that this project endorses it.
- You may say that your fork is "based on Hush" and link to this repository.
- Unmodified builds of a tagged release, or builds that only fix packaging for a distribution, may keep the name.

Questions go to the lead maintainer listed in [GOVERNANCE.md](GOVERNANCE.md).

## Open item before the repository goes public

The name "Hush" is already used by an MIT-licensed Safari content blocker for macOS and iOS ([oblador/hush](https://github.com/oblador/hush), "Hush Nag Blocker" on the App Store), which also markets itself on privacy. Decide whether to keep the name before the week 4 public release. Every brand string lives in `brand.rs`, so a rename is one file plus the icons.
