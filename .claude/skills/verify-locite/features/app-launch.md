# App launch

Starting Locite opens the main window titled `Locite`. On first run it creates an encrypted database and a keychain key. On later runs it reuses them. It writes a daily log that holds no document data.

## Sub-features

- `launch-window` shows the 960x680 main window with the `Locite` heading only when no folder is registered or the shortcut did not register; otherwise it stays hidden and the app has no Dock icon (`lsappinfo` type `UIElement`).
- `launch-storage` opens or creates the SQLCipher DB `locite.db` in the data dir, encrypted on disk.
- `launch-log` appends `app started` to the daily log with no ERROR line.
- `launch-shortcut` registers `Cmd+Shift+Space`, or logs `quick-search shortcut not registered` and still starts.

## How to get to it (user POV)

- Launch the app binary (dev: `pnpm tauri dev`; verification: `launch.sh`, which builds and runs `src-tauri/target/debug/locite`).

## Driving it with System Events + AX

Preconditions:

- No `locite` process is running.
- The user is around to answer a keychain prompt if one appears after a rebuild.

- **Launch.** Run `$S/launch.sh`. It prints `.verify/<timestamp>` and exits 0. `.verify/current/launch-ax.txt` contains `AXHeading title=Locite`.
- **Window.** Run `swift $S/windows.swift "$(cat .verify/current/pid)"`. The output includes `main <0|1> <id> 960x680` and `quick-search 0 <id> 680x460`.
- **Log.** Run `tail -n +$(( $(cat .verify/current/log-start) + 1 )) ~/Library/Logs/io.github.skyoo2003.locite/locite.$(date -u +%F).log`. It shows one `app started labels=[...]` line naming both `main` and `quick-search` (in either order) and no ` ERROR ` line.
- **Storage encrypted.** Run `head -c 16 ~/Library/Application\ Support/io.github.skyoo2003.locite/locite.db | xxd`. The bytes are not `53514c69746520666f726d6174203300` (`SQLite format 3\0`). This reads only the 16-byte header, never the content.
- **Proof.** Run `$S/shot.sh main main-window`. The capture shows the `Locite` heading.

## Gotchas

- `app started` is logged *before* storage opens. Treat the log line alone as "not ready yet". `launch.sh` waits for the AX heading.
- A new debug build triggers a keychain "allow access" prompt unless `LOCITE_DEV_SIGN_IDENTITY` is set (CONTRIBUTING.md). If one appears, `launch.sh` times out with `launch-timeout.png`. Ask the user to answer it. Never click it yourself.
- Never isolate the app with a fake `HOME`. macOS then offers to **reset the login keychain**. See SKILL.md.
- The log file name uses the UTC date. Around midnight UTC, a run's lines can span two files.
- `Cmd+Shift+Space` can be owned by another app (input source switching, Spotlight variants). Startup still succeeds, and the warning appears in the log.
