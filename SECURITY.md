# Security Policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue.

Include the version, the platform, and steps to reproduce. Never attach real private documents. Build a minimal sample file instead.

Maintainers aim to acknowledge a report within 7 days and to agree on a disclosure date with the reporter.

## Supported versions

Hush has not had a stable release yet. Until 1.0, only the latest release and `main` receive security fixes.

## In scope

Hush promises that documents never leave the device. These are treated as vulnerabilities:

- Any network request that carries document content, chunks, embeddings, queries, or file paths.
- Any outbound request that bypasses `AirGapGuard` or reaches a host other than the allowed license, model download, and update hosts.
- Reading a file outside a registered folder, including through `..` or symlinks.
- Plaintext copies of indexed data, including caches, temp files, and logs that contain document text, queries, file names, or keys.
- Decrypting the database without the keychain key.
- Answer text shown without a citation.
- Unlocking a paid capability without a valid license.

## Out of scope

- Attacks that need an already-compromised user account or OS.
- Removing the license check from a self-built copy. The GPL-3.0 license permits modifying the source.
