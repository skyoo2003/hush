---
name: verify-airgap
description: Model-check Hush's Air-Gap promise (no request leaves while Air-Gap is ON, and ON survives a relaunch) with TLA+ over every interleaving of toggles, requests, DB-write failures, and crashes. Use it when a change touches AirGapGuard, net::Client::get, set_air_gap, Db::set_setting / Db::setting, startup guard loading in lib.rs, or makes set_air_gap async or adds another caller of set_enabled.
---

# Verify the Air-Gap model

`verification/airgap/AirGap.tla` is a small TLA+ model of Air-Gap mode. TLC checks every order in which these can happen: the user flipping the switch, outbound requests, DB writes that fail, and the app quitting or crashing at any step. Unit tests check one order per test. This model checks all of them, within the bounds below.

## The rules

| Invariant | Plain rule | Harm if broken |
| --- | --- | --- |
| `CheckedOnlyWhenOff` | While the guard is ON, no request passes `AirGapGuard::check`. | Network traffic with Air-Gap ON (absolute rules 1–2). |
| `NoSendAfterOnAck` | Once `set_air_gap(true)` has returned Ok, no request leaves until the user turns Air-Gap OFF. | The user sees "ON" but a request still goes out. |
| `OnSurvivesRestart` | If the user was last told ON, the relaunched app is ON. | The app silently relaunches with the network open. |

## Does my change touch this?

Covered code, last confirmed against the model at commit **`3b1f4b2`**:

| Model action | Code |
| --- | --- |
| `ToggleStart` / `ToggleWrite` / `ToggleWriteFails` / `ToggleStore` | `src-tauri/src/security/airgap.rs` `AirGapGuard::set_enabled` (persist, then `on.send_replace`), called only from `src-tauri/src/commands.rs` `set_air_gap` |
| `ToggleDrain` | `AirGapGuard::set_enabled` with `on = true`: waits on `drained` until `in_flight` is 0, then returns |
| `ToggleWrite` is one atomic step | `src-tauri/src/storage/db.rs` `Db::set_setting`: one upsert under the `writer` Mutex |
| `RequestCheck` | `src-tauri/src/net.rs` `Client::get`: `self.guard.begin(purpose, host)?`, which counts the request in `in_flight`, then calls `check` |
| `RequestSend` | `Client::get`: the `send()` branch of the `tokio::select!` completes |
| `RequestCancel` | `Client::get`: the `turned_on()` branch of the `tokio::select!` wins, and the `InFlight` drop decrements `in_flight` |
| `Restart`, `Init` | `src-tauri/src/lib.rs` `builder` setup: `AirGapGuard::load(&db)`, which is ON unless the row is `"0"` (`Db::setting`) |
| `TogglesSerialized = TRUE` | `AirGapGuard::set_enabled` holds the `toggle` Mutex across the write, the store, and the drain, so two calls never overlap, whoever calls them. |

To see whether a change touches covered code:

```sh
git diff 3b1f4b2 -- src-tauri/src/security/airgap.rs src-tauri/src/net.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/storage/db.rs
```

Look for these changes in the diff: a new caller of `set_enabled`, `begin`, or `Client::get`, a change to the order inside any of them, a request path that skips the `turned_on` race, a new cache of the mode, or a new way to read the setting at startup. A change elsewhere in those files, such as a new table in `db.rs`, does not need a model change.

## Run

```sh
verification/airgap/run.sh        # about 6 s; -v also prints each counterexample
```

It needs Java 11 or later. The first run downloads `tla2tools.jar` v1.7.4 into `~/.cache/hush-tla/` and checks its SHA-256. The script checks each invariant separately on `AirGap.cfg`.

Bounds: 3 toggles with any ON/OFF values, 2 requests, 2 relaunches, and any starting row (`none`, `"0"`, `"1"`). Each row of output is `PASS` or `FAIL`, followed by `ok` when it matches the `EXPECTED` table in `run.sh`. Exit code 0 means everything matched.

Read each result with the pstack `/statechart` skill. A violation comes out as its shortest path in plain words, and a pass comes out as a chart. If its TLC install fails, give it the jar that `run.sh` downloaded through its `TLA2TOOLS` and `JAVA` environment variables. The model has about 31k states, which is over the chart budget, so expect bug paths only. To get a full chart, run it on a copy of the config with smaller bounds.

## Fixed findings

Both used to be expected FAILs. Each breaks again if its fix is removed, which is the first thing to try when checking that the model can still fail.

1. **`NoSendAfterOnAck`.** `Client::get` used to check the mode once, before `send().await`, so a request that passed while OFF still went out after the user turned Air-Gap ON. Now `set_enabled(true)` cancels in-flight requests through `turned_on` and returns only once `in_flight` is 0. Removing the `ToggleDrain` wait fails this invariant.
2. **`OnSurvivesRestart` with overlapping toggles.** OFF and ON both start → ON writes `"1"` → OFF writes `"0"` → OFF stores → ON stores, and the UI shows ON → relaunch loads `"0"` → OFF. The `toggle` Mutex now prevents this. Setting `TogglesSerialized = FALSE` fails this invariant and `NoSendAfterOnAck`.

## Real bug or modeling error?

When a row reads `UNEXPECTED`:

1. Run `run.sh -v`, or `/statechart`, and write the path as steps in code terms, using the table above.
2. Replay those steps against the real code, one step per action. Use a scratch test in `src-tauri/src/security/airgap.rs` or `net.rs` that calls `set_enabled`, `check`, and `AirGapGuard::load` in that order. For races, use threads with a temporary pause where the model splits a step, as in finding 2. Delete the scratch test afterwards.
3. If the code really takes those steps, it is a real bug. Report it with the path, and do not weaken the invariant.
4. If the code cannot take those steps, the model is wrong. Typical causes are an action that is atomic in the model but split in the code, or the reverse, or a missing guard. Fix the model so that it points at the code, and say what you changed.

`ERROR` means TLC failed, for example on a parse error or a missing `UNCHANGED` variable. The script prints the TLC output.

## Updating the model for an intended change

1. Change the matching action in `AirGap.tla`. Keep one model action per atomic step in the code, and keep the code-map comment at the top of the spec up to date.
2. Run `run.sh`. Update `EXPECTED` only for a result whose change you can explain. Never update it to silence a failure that you have not explained.
3. Check that the model can still fail. In a scratch copy, break the code path that the change touched and run `run.sh`. For example, make `RequestCheck` ignore `mem` (which must fail `CheckedOnlyWhenOff`), or make `ToggleWrite` leave `disk` unchanged (which must fail `OnSurvivesRestart`). The break must be caught.
4. Update the coverage table and the sync commit above. Move the commit forward only after you have confirmed that the model describes the covered code at that commit.

## Not covered

- Host and scheme checks, and the purpose→host table. Unit tests cover them (`check_matrix` and `blocked_requests_never_reach_reqwest`).
- Whether content, queries, or paths ever reach a request body (absolute rule 1).
- Network calls that bypass `net::Client`.
- A failed `AirGapGuard::load` at startup. Setup aborts, so no requests happen.
- SQLite durability itself: a committed write is assumed to survive a crash.
- Bounds beyond 3 toggles, 2 requests, and 2 relaunches.
