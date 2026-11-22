---
name: verify-hush
description: Launch and drive the real Hush desktop app (Tauri v2, macOS) to prove a change works the way a user sees it, through its main window and the Cmd+Shift+Space quick-search popup. Use it after touching the frontend, IPC commands, window/shortcut setup, or startup/storage. Use it when asked to verify, run, screenshot, or demo Hush, or when unit tests pass but you need proof from the running app.
---

# Verify Hush

Hush is a macOS desktop app: a Rust backend plus a React UI in two WKWebView windows. `main` is 960x680 and shows the app. `quick-search` is a 680x421 always-on-top popup, hidden at start, toggled by the global shortcut `Cmd+Shift+Space`. There is no web URL to point a browser at. Without the Tauri runtime, `invoke()` fails, so `pnpm dev` in a browser proves nothing. Drive the real binary through macOS Accessibility (System Events keystrokes plus an AX tree dump) and capture windows with `screencapture -l`.

All helpers live in `.claude/skills/verify-hush/scripts/`. Run them from the repo root. Each run writes its evidence to `.verify/<UTC timestamp>/` (git-ignored), and `.verify/current` points at the latest run.

## Read this first: you are driving the user's real data

Hush cannot run as an isolated instance (paths below use `BUNDLE_ID` from `src-tauri/src/brand.rs`, currently `com.hush.app`; the scripts read it from there):

- The data dir (`~/Library/Application Support/com.hush.app/`) and log dir (`~/Library/Logs/com.hush.app/`) come from the bundle ID.
- The SQLCipher DB key is always read from the user's **login keychain**.
- **Never set `HOME` to a scratch dir to isolate it.** The data dir moves, but macOS then reports the keychain missing and shows a **"Reset your keychain?"** dialog. Clicking Reset deletes all of the user's saved passwords. If any keychain dialog appears, never click it: run `cleanup.sh`, then tell the user.
- Ad-hoc signed rebuilds make macOS ask "hush wants to use your confidential information stored in … keychain" again. `launch.sh` starts the app through `scripts/dev-sign.sh`, which re-signs with `$HUSH_DEV_SIGN_IDENTITY` when it is set (see CONTRIBUTING.md), and then no prompt appears after rebuilds. If the variable is unset or the prompt appears anyway, only the user may answer it. `launch.sh` times out after 120s with a screenshot and tells you to ask the user.
- `launch.sh` refuses to start if any `hush` process is already running. Never drive or kill an instance you did not start.
- Evidence can show the user's real UI state. Once ingestion lands, that includes document text. It stays local in `.verify/`. Never attach it to issues, PRs, or chat (see the governance rules in CLAUDE.md).

Requirements: the terminal app needs Accessibility and Screen Recording permission (System Settings > Privacy & Security). `doctor.sh` checks Accessibility.

## Launch

```sh
.claude/skills/verify-hush/scripts/launch.sh            # pnpm tauri build --debug --no-bundle, then start
.claude/skills/verify-hush/scripts/launch.sh --no-build # reuse src-tauri/target/debug/hush
```

- The build embeds `dist/`, so no Vite server or port 1420 is involved. A cold build takes about 40s, and an incremental one a few seconds.
- The app is ready when the log (`~/Library/Logs/com.hush.app/hush.<UTC date>.log`) has a new `app started` line **and** the main window's AX tree answers with `AXHeading title=Hush`. The log line alone is not enough: it is written *before* storage opens, and a keychain prompt can block setup after it. While setup runs, the main thread is blocked and AX returns nothing, so the rendered heading means setup returned. A setup failure logs `storage open failed` and the process exits.
- On success the script prints the run dir. On failure it exits non-zero with the reason: build failed, `storage open failed`, the app exited, or a 30s timeout plus `launch-timeout.png`.
- An empty data dir is fine. The app creates the DB and the keychain key on first launch. On a machine that already has a key, it reuses it.

## Doctor

```sh
.claude/skills/verify-hush/scripts/doctor.sh
```

This is read-only. It checks that the recorded pid is alive, that no foreign `hush` pids exist, that the binary is not older than `src/` or `src-tauri/src/`, that the log has `app started` since launch (and shows the last WARN/ERROR), and that System Events is reachable. It also lists both windows as `<label> <onscreen> <CGWindowID> <size>`. Run it first whenever anything looks off. `doctor: NOT ok` means do not drive.

## Drive

| Need | Command |
| --- | --- |
| Window state | `swift .claude/skills/verify-hush/scripts/windows.swift "$(cat .verify/current/pid)"` gives `main 1 2909 960x680` and `quick-search 0 2910 680x421` |
| UI tree (ARIA labels, input values, headings) | `swift .claude/skills/verify-hush/scripts/ax.swift "$(cat .verify/current/pid)"` |
| Toggle quick-search | `osascript -e 'tell application "System Events" to key code 49 using {command down, shift down}'` |
| Type into the focused field | `osascript -e 'tell application "System Events" to keystroke "20261122"'` |
| Escape | `osascript -e 'tell application "System Events" to key code 53'` |
| Window screenshot | `.claude/skills/verify-hush/scripts/shot.sh <main\|quick-search> <name>` writes `.verify/current/<name>.png` |
| Scripted journey | `.claude/skills/verify-hush/scripts/journey-quick-search.sh` |

Stable handles: the search input is `AXTextField/AXSearchField desc=Search` (`aria-label="Search"`), and the main heading is `AXHeading title=Hush`. Both windows have the title `Hush`, so identify them by size or by `windows.swift` label, never by title.

Rules:

- Keystrokes go to whatever app is frontmost. Before typing, confirm that the frontmost pid is ours: `osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true'`.
- WebKit builds its AX tree lazily. The first `ax.swift` call after a window appears often shows an empty `AXGroup`. Poll a few times, as `journey-quick-search.sh` does, before you call it a failure.
- Type digits or other non-words. Words trigger the macOS autocorrect bubble, and the next Escape closes the bubble instead of reaching the page.

## Evidence

A proof is: the action you took, the resulting UI state, and the side effect, all from the real user path.

- **UI state:** an `ax.swift` dump (text you can grep) plus a `shot.sh` window capture, taken after each meaningful step, not only at the end.
- **Side effects:** new lines in the app log since `log-start` (`tail -n +$(( $(cat .verify/current/log-start) + 1 )) ~/Library/Logs/com.hush.app/hush.$(date -u +%F).log`), the DB file's existence and header (it must not start with `SQLite format 3`), and window visibility from `windows.swift`.
- Never prove a feature by calling an IPC command, a test-only path, or a Rust unit test. Those are tests, not verification. If a feature has no UI entry point yet, report it as unreachable. Do not substitute another path.
- Logs never contain queries or document text (rule 5). If your probe string shows up in the log, report that as a bug.
- Write a short `.txt` summary next to the images in the run dir. `journey-quick-search.sh` writes `quick-search.txt`.

## Cleanup

```sh
.claude/skills/verify-hush/scripts/cleanup.sh
```

This stops only the pid recorded in `.verify/current/pid`, after checking that its command path is our binary. It never kills by process name. Evidence in `.verify/<run>/` is kept, and the script prints where. It never deletes the user's DB, logs, or keychain item. Run it after every failed attempt too.

## Features

See [features/README.md](features/README.md) for the feature map and a recipe for each feature.
