# Verify the Air-Gap yourself

Locite promises that document content, chunks, embeddings, queries, and file paths never leave your Mac. CI checks this on every change (see [Automated checks](#automated-checks)). This page is the manual check: you cut the network, use every feature, and watch for any connection attempt. It takes about 15 minutes, and you need no Rust knowledge.

## What you need

- A Locite build (a release, or `pnpm tauri build --debug --no-bundle` from source).
- A per-app firewall that shows every connection attempt: [LuLu](https://objective-see.org/products/lulu.html) (free, open source) or Little Snitch. Set it to alert on every new connection.
- `nettop`, which ships with macOS.
- A folder of made-up test files. Do not use real documents.

## Procedure

1. Install the embedding model first (Settings > Security > Download, or Import with Air-Gap ON). After that, Locite needs no network for anything below.
2. Quit Locite. Turn Air-Gap ON in Settings > Security if it is off.
3. Turn off Wi-Fi and unplug Ethernet.
4. Start Locite and find its process id: `pgrep -x locite`.
5. In a terminal, start recording its network activity and leave it running:

   ```sh
   nettop -p "$(pgrep -x locite)" -L 0 -s 1 -x > locite-nettop.csv
   ```

6. Use every feature, in this order:
   - Add your test folder, wait for "Up to date.", then edit, rename, and delete one file and wait for the count to update.
   - Press Cmd+Shift+Space and search. Try operators: `"exact phrase"`, `ext:txt`, `type:pdf`, `folder:<name>`, `after:2024`.
   - On a result: Enter (open), Cmd+Enter (show in Finder), Cmd+C (copy citation).
   - In Settings: disable and re-enable the folder, edit the exclude patterns, Retry a failed file, change the index schedule, Pause and Resume from the tray, and change the shortcut.
   - In Settings > Security: Import a model file from disk. Try Download, which must stay disabled while Air-Gap is ON.
   - Remove the test folder.
7. Turn Wi-Fi back on **with Air-Gap still ON** and repeat step 6. This is the case that matters: the network is there, and Locite still must not use it.
8. Stop `nettop` (Ctrl+C) and quit Locite.

## What passes

- LuLu or Little Snitch shows no connection attempt from `locite` in either pass.
- `locite-nettop.csv` holds only header lines (`time,,interface,state,...`) and no row naming a socket (`tcp4`, `tcp6`, `udp4`, `udp6`).
- `lsof -nP -a -p "$(pgrep -x locite)" -i` prints nothing while Locite runs.

Any connection is a bug. Report it through [GitHub private vulnerability reporting](../SECURITY.md), not a public issue, with the remote host and the step you were on. Do not attach document content or file paths.

The only connections Locite can make are a model download and an update check, and only with Air-Gap OFF after you choose to (absolute rule 2). macOS itself may make connections on Locite's behalf, for example code-signing checks at first launch, under `trustd` or `syspolicyd`. Those are not Locite and do not appear in its `nettop` output.

## Automated checks

CI runs these on every pull request:

| Check | What it proves |
| --- | --- |
| `scripts/check-net-usage.sh` | Rust code outside `src-tauri/src/net` never names `reqwest`, `hyper`, `ureq`, `std::net`, `tokio::net`, `TcpStream`, `TcpListener`, or `UdpSocket`, and every networking crate reaches the app only through `reqwest`. It writes [network-deps.md](network-deps.md). |
| `cargo deny` and `cargo audit` | No dependency has a known vulnerability. |
| `index_search_and_copy_make_no_network_call_with_air_gap_on` (`src-tauri/src/lib.rs`) | With Air-Gap ON, indexing, search, and copying a citation call `net::Client` zero times. |
| `scripts/check-dist-urls.mjs` | The frontend build contains no external URL outside an allow list (XML namespace names, React error text, and the site and sponsor URLs from `src/brand.ts`). The webview's CSP (`connect-src ipc: http://ipc.localhost`) also blocks any fetch. |

## Recorded runs

### 2026-11-25, macOS 26.6.2, Apple Silicon, debug build of `main` plus this change

Run by the maintainer's coding agent, so only partly done:

- Done: started the app and recorded `nettop -p <pid> -s 1 -x` for 90 one-second samples. Over that time the app rescanned its registered folders at launch, ran a quick search that returned results, and copied a citation with Cmd+C. **Result: every sample held only the header line, so no socket was open.** `lsof -i` for the process was empty before and after. The query text did not appear in the log. A positive control (`nc -l` on localhost) showed `nettop` lists a socket when one exists.
- Not done: Wi-Fi off (it would have cut the agent's own connection); LuLu or Little Snitch (neither is installed); adding a folder, open, reveal, and every Settings control (the tray icon was hidden behind the menu-bar overflow, so the main window could not be reached); confirming the Air-Gap switch position.

The full procedure above still needs one run by a person. Add it here as its own entry: date, macOS version, Locite version, firewall used, and the result of each pass.
