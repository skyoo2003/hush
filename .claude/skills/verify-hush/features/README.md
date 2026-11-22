# Hush verification map

This directory is the maintained source for verifying Hush's user-facing behavior. Read this index before driving the app, then use the matching feature file as the recipe. Paths are relative to the repo root, and `S=.claude/skills/verify-hush/scripts`.

## Baseline preconditions

- No other `hush` process is running. `$S/launch.sh` refuses otherwise.
- Launch with `$S/launch.sh` (or `--no-build` when `doctor.sh` says the binary is up to date), then require `$S/doctor.sh` to print `doctor: ok`.
- The instance runs on the user's real data dir and login keychain. There is no disposable data dir and no seed data. Read SKILL.md's "you are driving the user's real data" section before the first run.
- The quick-search popup starts hidden (`quick-search 0` in `windows.swift`).

## Driving conventions

- Prefer AX roles and accessible names (`AXSearchField desc=Search`, `AXHeading title=Hush`, `AXSwitch title=Air-Gap mode`) over coordinates. Identify windows by the `windows.swift` label, never by title (both are `Hush`).
- Confirm that the frontmost pid is ours before every keystroke.
- Leave the app as you found it: popup hidden, no setting changed. Undo any mutation and prove that the undo worked too.

## Proof and skip reporting

- Capture the action and the resulting state: an `ax.swift` dump plus a `shot.sh` capture per step, saved in `.verify/current/`.
- Prove side effects through the log (new lines since `log-start`), window state, and the DB file, never through IPC calls or tests.
- When a feature has no UI entry point yet, report it as **unreachable**, with the attempted path. Do not report it as verified through another path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then has exactly four H2 sections, in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with System Events + AX`, and `Gotchas`.

## Features

- [App launch](./app-launch.md): the main window opens, and the encrypted DB and log are created. Status: drivable.
- [Quick search popup](./quick-search.md): the global shortcut shows the popup, the user types, and Escape clears and hides it. Status: drivable, proven by `journey-quick-search.sh` (including the shortcut toggle).
- [Air-Gap toggle](./air-gap.md): turn network access for license, model, and update checks on or off. Status: drivable, proven by `journey-air-gap.sh`.
- [Search results and citations](./search-results.md): run a query, open the source, copy a citation. Status: **unreachable**, the commands return `not_implemented` and there is no results UI.
