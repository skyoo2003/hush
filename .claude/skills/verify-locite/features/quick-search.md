# Quick search popup

From anywhere in macOS, `Cmd+Shift+Space` shows a small always-on-top Locite popup with its search field focused. Escape clears the query and hides the popup. The next time it opens, the field is empty.

## Sub-features

- `qs-show` shows the hidden popup and makes it frontmost with the shortcut.
- `qs-type` sends typed text into the `Search` field.
- `qs-escape` makes Escape clear the field and hide the popup (through the `hide_window` command).
- `qs-reopen-empty` opens the popup again with an empty field.
- `qs-toggle` hides the visible popup when the shortcut is pressed again (the query is kept, unlike Escape).

## How to get to it (user POV)

- Press `Cmd+Shift+Space` from any app.
- There is no menu or main-window entry point yet.

## Driving it with System Events + AX

Preconditions:

- `doctor.sh` prints `doctor: ok`, and `windows.swift` shows `quick-search 0`.
- The log has no `quick-search shortcut not registered` line since launch.

- **Whole journey.** Run `$S/journey-quick-search.sh`. Exit 0 and a last line of `PASS: quick-search show/type/escape/clear/toggle`. Evidence: `quick-search.txt`, `quick-search-ax.txt`, `quick-search-typed.png`, `quick-search-reopen-ax.txt`, and `quick-search-reopened.png` in `.verify/current/`.
- **Show.** Run `osascript -e 'tell application "System Events" to key code 49 using {command down, shift down}'`. `windows.swift` shows `quick-search 1`, and the frontmost pid equals `.verify/current/pid`.
- **Type.** Run `osascript -e 'tell application "System Events" to keystroke "20261122"'`. `ax.swift` (polled) contains `AXTextField/AXSearchField title=Search desc=Search value=20261122`.
- **Escape.** Run `osascript -e 'tell application "System Events" to key code 53'`. `windows.swift` shows `quick-search 0`.
- **Reopen empty.** Press the shortcut again. `ax.swift` shows `AXSearchField` with no `value=`. Press Escape to restore the baseline.
- **Toggle.** With the popup visible, press the shortcut. `windows.swift` shows `quick-search 0`. The journey covers this as step 5.
- **Privacy check.** After the journey, `grep 20261122` on the new log lines must find nothing (rule 5).

## Gotchas

- Typing a word (for example `verify-probe`) pops up the macOS autocorrect bubble, and the first Escape only closes the bubble. Use digits.
- The first `ax.swift` call after the popup shows often returns only `AXGroup`, because WebKit builds the AX tree lazily. Poll with `ax_until` from `lib.sh`.
- A hidden popup is missing from the AX window list entirely. Use `windows.swift` for visibility, not AX.
- If the shortcut does nothing, check the log for `quick-search shortcut not registered` before blaming the frontend.
- Keystrokes go to the frontmost app. If Locite is not frontmost, stop: typing would land in the user's other apps.
