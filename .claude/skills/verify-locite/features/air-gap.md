# Air-Gap toggle

The user turns Air-Gap mode on (the default) or off. While it is on, the app makes no outbound requests, including model downloads and update checks. The setting survives a restart.

## Sub-features

- `airgap-default-on` makes Air-Gap on by default on a fresh install.
- `airgap-toggle` switches it from the main window.
- `airgap-persist` keeps the choice after a restart.

## How to get to it (user POV)

- The main window shows an `Air-Gap mode` switch under the heading, with one line saying what ON or OFF allows.

## Driving it with System Events + AX

Preconditions:

- `doctor.sh` prints `doctor: ok`.
- A relaunch must not show a keychain prompt: `LOCITE_DEV_SIGN_IDENTITY` is set, or the user chose "Always Allow" for this binary.

- **Whole journey.** Run `$S/journey-air-gap.sh`. Exit 0 and a last line of `PASS: air-gap toggle/persist/restore (<orig> -> <flipped> -> <orig>)`. It presses the switch through AX (`press.swift`, no keystrokes), relaunches with `--no-build`, checks the value, then restores the user's value and relaunches again. Evidence: `air-gap.txt`, `air-gap-{start,flipped,relaunched,restored,final}-ax.txt`, and the matching `.png` files in the starting run dir.
- **Find the switch.** `ax.swift` shows `AXCheckBox/AXSwitch title=Air-Gap mode value=1` (ON) or `value=0` (OFF).
- **Flip it.** `swift $S/press.swift "$(cat .verify/current/pid)" AXCheckBox "Air-Gap mode"`. Poll `ax.swift` until `value=` changes. No `role=alert` text may appear.

## Gotchas

- Do not "verify" this feature by invoking `set_air_gap` over IPC or by citing the `set_air_gap_persists_and_get_settings_reads_it` unit test. Neither is a user path.
- If the journey fails midway, the user's setting may be left flipped. Read `air-gap.txt`, then restore it with `press.swift` and prove it.
- This is the user's real setting. Any verification that flips it must restore it.
- Network side effects are not observable with this harness. Proving "no outbound requests" needs a packet-level tool and is out of scope.
