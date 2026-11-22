# Governance

Hush is a GPL-3.0-only open source project. This file says who decides what, and how.

## Roles

- **Users** run the app and report problems through issues.
- **Contributors** send pull requests. Anyone can contribute under the terms in [CONTRIBUTING.md](CONTRIBUTING.md).
- **Maintainers** review and merge pull requests, triage issues, cut releases, and enforce the [Code of Conduct](CODE_OF_CONDUCT.md).
- **Lead maintainer** has the final say when maintainers disagree, and owns the release keys, the repository settings, and the license provider account.

## Maintainers

| Name | GitHub | Role |
| --- | --- | --- |
| Sung-Kyu Yoo | [skyoo2003](https://github.com/skyoo2003) | Lead maintainer |

[`.github/CODEOWNERS`](.github/CODEOWNERS) must list the same people.

## How decisions are made

1. Most changes are decided in the pull request. One maintainer approval and green CI are enough to merge.
2. A change to an absolute rule in [CLAUDE.md](CLAUDE.md), the license, this file, or the tier model needs an issue first, with at least 7 days for comment, and approval from the lead maintainer.
3. An architecture decision is recorded as an ADR in [docs/decisions.md](docs/decisions.md) in the same pull request that makes it.
4. If maintainers cannot agree, the lead maintainer decides and writes down the reason in the issue or pull request.

## Becoming a maintainer

A contributor with a steady record of reviewed, merged work can be nominated by any maintainer. The lead maintainer approves the nomination in a pull request that updates the table above and `CODEOWNERS`. A maintainer who is inactive for 12 months moves to emeritus status in the same way.

## Releases

Maintainers tag releases from `main` after CI passes. Every release ships its full corresponding source under GPL-3.0-only, including the code behind paid tiers.

## License and contributions

- The project is licensed under [GPL-3.0-only](LICENSE).
- Contributions are accepted under the same license and signed off under the [Developer Certificate of Origin](https://developercertificate.org/). There is no CLA, so copyright stays with each contributor and the project cannot be relicensed without their consent.
- Every dependency must use a GPL-3.0-compatible license. `cargo deny check licenses` enforces this for Rust crates in CI.

## Contact

- Security issues: GitHub private vulnerability reporting, described in [SECURITY.md](SECURITY.md).
- Conduct issues: TODO, add a private contact address before the first public release. Until then, contact the lead maintainer through their GitHub profile.
- Everything else: GitHub issues.
