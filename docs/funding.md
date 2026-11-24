# Funding

Locite is free and open source. Sponsors pay for what it costs to maintain it: code signing, notarization, and maintenance time.

## Sponsorship does not affect features

Every feature works the same for every user. Sponsoring unlocks nothing, and not sponsoring locks nothing. The app has no usage limits, upgrade prompts, payment code, or license checks (absolute rule 8 in [CLAUDE.md](../CLAUDE.md)).

## Channels

- [GitHub Sponsors](https://github.com/sponsors/skyoo2003): the main channel, for monthly sponsorship.
- [Buy Me a Coffee](https://buymeacoffee.com/skyoo2003): one-time support for people without a GitHub account.

Both appear behind the repository's Sponsor button through [`.github/FUNDING.yml`](../.github/FUNDING.yml). The build fails if the app's links (`SPONSOR_URLS` in `src-tauri/src/brand.rs`) differ from that file.

Inside the app, sponsorship appears in three places only: Settings > Sponsor, the tray's Sponsor menu, and a one-time banner at the bottom of the main window after 100 searches or 100 indexed documents. Once dismissed, the banner never returns. A link opens in the external browser. With Air-Gap on, the app opens nothing and shows a QR code it generates locally. The app makes no network request for sponsorship.

## Goals

**Current goal:** Stage 1, the Apple Developer Program annual fee (code signing and notarization) and the project domain.

| Stage | Covers | Amount |
| --- | --- | --- |
| 1 | Apple Developer Program annual fee (code signing and notarization) and the domain | TODO before launch |
| 2 | N hours of maintenance per week | TODO before launch |

The app shows the `**Current goal:**` line above. It is copied into the app at build time (`src/funding.ts`), so changing it takes effect in the next release. The app never fetches it.

## Quarterly report

Each quarter adds one row. Totals only. No sponsor names, handles, or per-person amounts.

| Quarter | Received | Platform fees | Spent | Spent on |
| --- | --- | --- | --- | --- |
| | | | | |

## Thank-you list

Thanks are symbolic:

- A name in the README thank-you list.
- An in-app thank-you badge, turned on by the user with the "I sponsored" switch. It is self-reported and never verified. The OLED dark and Terminal themes it points to are selectable by everyone.
- Votes on the roadmap.

A sponsor is listed in the README only after they agree to it in writing (a reply on the platform, an issue, or an email). Listing shows the name or handle they choose, never an amount, a tier, or contact details. A sponsor can ask to be removed at any time, and the next commit removes them. One-time and monthly sponsors are listed the same way.

## Before opening the accounts

Platform fees, payout methods, minimum payouts, and tax reporting differ by platform and country and change over time. Check each platform's current terms for payouts to Korea before signing up, and record the fees in the quarterly report.
