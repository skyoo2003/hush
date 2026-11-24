# Search results and citations

The user types a query in the quick-search popup and gets results from their indexed folders. Each card shows an icon, the file name, the page or section, the folder label, the modified date, and a highlighted snippet. Operators such as `ext:txt` show as read-only chips. From a result the user can open the file, reveal it in Finder, or copy a citation.

## Sub-features

- `search-query` shows ranked results and operator chips; empty input shows the operator hint.
- `search-open-source` (Enter) opens the file in its default app and hides the popup.
- `search-reveal` (Cmd+Enter) selects the file in Finder.
- `search-copy-citation` (Cmd+C with no text selected in the input) puts `[file, p.N] "excerpt"` on the clipboard.
- `search-insufficient` shows 「제공된 로컬 문서에서 확인되지 않음」 when nothing supports an answer. Not built yet.

## How to get to it (user POV)

- Register a folder of synthetic files from the main window, wait for "Up to date.", then press the shortcut and type.

## Driving it with System Events + AX

Preconditions: `doctor.sh` prints `doctor: ok`, and a registered folder holds synthetic files with a digit token (for example `20261122`), since words trigger autocorrect.

- Query: type `20261122 ext:txt`. `ax.swift` shows `AXGroup value=<Extension label>: txt` under the input and an `AXStaticText title=<file>` inside the results list.
- Copy: save `pbpaste`, press Cmd+C, compare `pbpaste` with `[<file>...] "..."`, then restore the clipboard.
- Reveal: press Cmd+Enter; `lsappinfo front` is Finder and `tell application "Finder" to get name of item 1 of (get selection)` is the file. Close that Finder window after.
- Open: press Enter; the default app has the file open and the popup is hidden. Close the document after.
- No match: `folder:nomatch 20261122` shows the no-results text.

## Gotchas

- The UI follows the system language, so labels may be Korean. Grep for values, not English labels.
- macOS turns a typed `"` into `“` or `”`; the parser accepts both.
- The first `ax.swift` call after typing can come back empty; poll again.
- The query must never appear in the log (rule 5). Grep the new log lines for the probe after every run.
