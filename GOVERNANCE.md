# Governance

Locite is a free, GPL-3.0-or-later open source project funded by sponsors. This file says who decides what, and how.

## Roles

- **Users** run the app and report problems through issues.
- **Contributors** send pull requests. Anyone can contribute under the terms in [CONTRIBUTING.md](CONTRIBUTING.md).
- **Maintainers** review and merge pull requests, triage issues, cut releases, and enforce the [Code of Conduct](CODE_OF_CONDUCT.md).
- **Lead maintainer** has the final say when maintainers disagree, and owns the release keys, the repository settings, and the sponsorship accounts.

## Maintainers

| Name | GitHub | Role |
| --- | --- | --- |
| Sung-Kyu Yoo | [skyoo2003](https://github.com/skyoo2003) | Lead maintainer |

[`.github/CODEOWNERS`](.github/CODEOWNERS) must list the same people.

## How decisions are made

1. Most changes are decided in the pull request. One maintainer approval and green CI are enough to merge.
2. A change to an absolute rule in [CLAUDE.md](CLAUDE.md), the license, this file, or the funding model needs an issue first, with at least 7 days for comment, and approval from the lead maintainer.
3. An architecture decision is recorded as an ADR in [docs/decisions.md](docs/decisions.md) in the same pull request that makes it.
4. If maintainers cannot agree, the lead maintainer decides and writes down the reason in the issue or pull request.

## Becoming a maintainer

A contributor with a steady record of reviewed, merged work can be nominated by any maintainer. The lead maintainer approves the nomination in a pull request that updates the table above and `CODEOWNERS`. A maintainer who is inactive for 12 months moves to emeritus status in the same way.

## Releases

Releases are built by [`.github/workflows/release.yml`](.github/workflows/release.yml) ([ADR-021](docs/decisions.md)):

1. A maintainer bumps the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`, adds its CHANGELOG.md section, and merges that to `main` with green CI.
2. The lead maintainer pushes the tag `vX.Y.Z` on that `main` commit. A tag ruleset lets only repository admins create, move, or delete `v*` tags.
3. The workflow checks the tag and builds the app without secrets. After the lead maintainer approves the deployment in the `release` environment, the app is signed with the project's self-signed release certificate. Releases are not notarized until the project can pay for the Apple Developer Program ([ADR-021](docs/decisions.md)).
4. After a second approval, the updater archive is signed with the updater key and `latest.json` is written ([ADR-022](docs/decisions.md)). The draft release carries the DMG, the updater archive and `latest.json`, the full corresponding source archive of the tag, the CycloneDX SBOM, `THIRD_PARTY_NOTICES.txt`, and `SHA256SUMS`, each with a build provenance attestation.
5. A maintainer checks that the draft DMG opens on a clean Mac, publishes it, and runs `scripts/bump-homebrew-tap.sh X.Y.Z` to update the cask in [skyoo2003/homebrew-tap](https://github.com/skyoo2003/homebrew-tap).

Every release ships its full corresponding source under GPL-3.0-or-later. Releases go to GitHub Releases and Homebrew only.

## License and contributions

- The project is licensed under [GPL-3.0-or-later](LICENSE).
- Contributions are accepted under the same license and signed off under the [Developer Certificate of Origin](https://developercertificate.org/). There is no CLA, so copyright stays with each contributor and the project cannot be relicensed without their consent.
- Every dependency must use a GPL-3.0-compatible license. CI enforces this with `cargo deny check` for Rust crates and `scripts/check-npm-licenses.mjs` for pnpm packages, both reading the allow list in `src-tauri/deny.toml`. CI also checks SPDX headers, DCO sign-off on pull requests, and the full history for secrets.

## Funding

- Locite is free. Sponsorship never changes what the app does (absolute rule 8 in [CLAUDE.md](CLAUDE.md)).
- Channels: GitHub Sponsors (monthly) and Buy Me a Coffee (one-time), both listed in [`.github/FUNDING.yml`](.github/FUNDING.yml).
- Goals are public and numeric. Totals and spending are published every quarter in [docs/funding.md](docs/funding.md), without sponsor personal data.

## Maintenance time

- Issues and pull requests are triaged once a week, in a fixed one-hour slot.
- Every external pull request gets a review against the absolute rules before merge.
- A security review of the week's changes runs at the end of each development week.
- What is supported is written down in [SUPPORT.md](SUPPORT.md).

## Contact

- Security issues: GitHub private vulnerability reporting, described in [SECURITY.md](SECURITY.md).
- Conduct issues: email the lead maintainer at skyoo2003@gmail.com. Reports are kept confidential.
- Everything else: GitHub issues.
