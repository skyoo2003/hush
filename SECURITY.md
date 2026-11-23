# Security Policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue.

Include the version, the platform, and steps to reproduce. Never attach real private documents. Build a minimal sample file instead.

Target response times:

| Step | Target |
| --- | --- |
| Acknowledge the report | 7 days |
| Confirm or reject it, with a severity | 14 days |
| Release a fix for a confirmed high or critical issue | 30 days |

Maintainers agree on a disclosure date with the reporter, publish a GitHub Security Advisory with the fix, and credit the reporter unless they ask not to be.

## Supported versions

Locite has not had a stable release yet. Until 1.0, only the latest release and `main` receive security fixes. After 1.0, the latest release receives fixes on the macOS versions listed in [SUPPORT.md](SUPPORT.md).

## In scope

Locite promises that documents never leave the device. These are treated as vulnerabilities:

- Any network request that carries document content, chunks, embeddings, queries, or file paths.
- Any outbound request that bypasses `AirGapGuard` or reaches a host other than the allowed model download and update hosts, or any telemetry.
- Reading a file outside a registered folder, including through `..` or symlinks.
- Plaintext copies of indexed data, including caches, temp files, and logs that contain document text, queries, file names, or keys.
- Decrypting the database without the keychain key.
- Answer text shown without a citation.

## Out of scope

- Attacks that need an already-compromised user account or OS.
- Unofficial builds and forks. Report those to their maintainers.
