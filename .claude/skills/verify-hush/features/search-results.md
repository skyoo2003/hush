# Search results and citations

The user types a query in the quick-search popup and gets results from their indexed folders. Each result shows the file, page, location, and excerpt. From a result they can open the source file or copy a citation.

## Sub-features

- `search-query` shows ranked results for a query in the popup.
- `search-open-source` opens the original file at the cited location.
- `search-copy-citation` copies a citation to the clipboard.
- `search-insufficient` shows 「제공된 로컬 문서에서 확인되지 않음」 when nothing supports an answer.

## How to get to it (user POV)

- Type in the quick-search popup (see [quick-search.md](./quick-search.md)). **Not built yet.** The input only holds text: no request is sent on typing or Enter, `search`, `open_source`, and `copy_citation` return `not_implemented`, and there is no folder registration UI to index anything.

## Driving it with System Events + AX

Preconditions:

- `doctor.sh` prints `doctor: ok`.
- A registered folder with indexed fixture documents. **Not possible today**, and there is no seed path. When ingestion lands, add a fixture folder from `tests/fixtures/` (synthetic content only, never real documents) and record how to register it here.

- **Query.** Show the popup, type a fixture term, and press Return (`key code 36`). Today `ax.swift` shows no results list, so report `search: unreachable (not implemented)`.

## Gotchas

- The evidence will contain document excerpts once this works. Use only synthetic fixtures, and keep `.verify/` local.
- The query must never appear in the log (rule 5). Grep the new log lines for the probe after every search run.
