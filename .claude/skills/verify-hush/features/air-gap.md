# Air-Gap toggle

The user turns Air-Gap mode on (the default) or off. While it is on, the app makes no outbound requests, including license, model download, and update checks. The setting survives a restart.

## Sub-features

- `airgap-default-on` makes Air-Gap on by default on a fresh install.
- `airgap-toggle` switches it from the main window.
- `airgap-persist` keeps the choice after a restart.

## How to get to it (user POV)

- Planned: a toggle in the main window's settings. **Not built yet.** The main window shows only the `Hush` heading. `get_settings` is a stub, and only the `set_air_gap` IPC command exists.

## Driving it with System Events + AX

Preconditions:

- `doctor.sh` prints `doctor: ok`.

- **Find the toggle.** Run `swift $S/ax.swift "$(cat .verify/current/pid)" | grep -iE 'air|switch|checkbox'`. Today there is no match, so report `air-gap: unreachable (no UI entry point)` and stop.
- **When the UI lands:** find the toggle by its accessible name, flip it with `AXPress` (add a helper), and prove it with the AX state plus a restart (`cleanup.sh`, `launch.sh --no-build`) that shows the same state. Then flip it back to the user's original value and prove that too.

## Gotchas

- Do not "verify" this feature by invoking `set_air_gap` over IPC or by citing the `set_air_gap_persists` unit test. Neither is a user path.
- This is the user's real setting. Any verification that flips it must restore it.
- Network side effects are not observable with this harness. Proving "no outbound requests" needs a packet-level tool and is out of scope.
