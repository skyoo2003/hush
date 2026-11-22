---------------------------- MODULE AirGap ----------------------------
(* Air-Gap mode against concurrent requests, toggles, and app restarts.
   The code map and the assumptions are in .claude/skills/verify-airgap/SKILL.md.

   Code steps that are one atomic step here:
   - ToggleWrite  = Db::set_setting, one statement under the writer Mutex  (storage/db.rs)
   - ToggleStore  = enabled.store(SeqCst), then set_air_gap returns Ok     (security/airgap.rs)
   - RequestCheck = AirGapGuard::check, one SeqCst load                    (net.rs Client::get)
   - RequestSend  = sent.fetch_add, then reqwest send                      (net.rs Client::get)
   - Restart      = AirGapGuard::load: ON unless the row is "0"            (lib.rs setup) *)
EXTENDS Naturals

CONSTANTS
    Toggles,          \* set_air_gap calls
    Requests,         \* Client::get calls
    MaxRestarts,
    TogglesSerialized \* TRUE: a sync #[tauri::command] runs inline on the main thread

VARIABLES
    disk,     \* the "airgap_enabled" settings row: "none" (never written), "0", or "1"
    mem,      \* AirGapGuard.enabled
    tpc,      \* per toggle: "idle" | "write" | "store" | "done"
    tval,     \* per toggle: the value the user asked for
    rpc,      \* per request: "idle" | "checked" | "blocked" | "sent"
    restarts,
    uiOn,     \* what the user was last told: the result of the last set_air_gap that returned Ok
    passedWhileOn,   \* a check let a request through while the guard was ON
    sentAfterOnAck,  \* a request left while the user was told ON and had not asked for OFF
    onLostOnRestart  \* a relaunch came up OFF while the user was told ON and had not asked for OFF

vars == <<disk, mem, tpc, tval, rpc, restarts, uiOn, passedWhileOn, sentAfterOnAck, onLostOnRestart>>

Load(d) == d /= "0"
Busy(t) == tpc[t] \in {"write", "store"}
\* The user was told ON and no request to turn it OFF is in flight.
UserExpectsOn == uiOn /\ \A t \in Toggles : ~(Busy(t) /\ ~tval[t])

Init ==
    /\ disk \in {"none", "0", "1"}
    /\ mem = Load(disk)
    /\ tpc = [t \in Toggles |-> "idle"]
    /\ tval \in [Toggles -> BOOLEAN]
    /\ rpc = [r \in Requests |-> "idle"]
    /\ restarts = 0
    /\ uiOn = mem
    /\ passedWhileOn = FALSE
    /\ sentAfterOnAck = FALSE
    /\ onLostOnRestart = FALSE

\* The user flips the switch and set_air_gap starts.
ToggleStart(t) ==
    /\ tpc[t] = "idle"
    /\ TogglesSerialized => \A u \in Toggles : ~Busy(u)
    /\ tpc' = [tpc EXCEPT ![t] = "write"]
    /\ UNCHANGED <<disk, mem, tval, rpc, restarts, uiOn, passedWhileOn, sentAfterOnAck, onLostOnRestart>>

\* set_enabled persists first.
ToggleWrite(t) ==
    /\ tpc[t] = "write"
    /\ disk' = IF tval[t] THEN "1" ELSE "0"
    /\ tpc' = [tpc EXCEPT ![t] = "store"]
    /\ UNCHANGED <<mem, tval, rpc, restarts, uiOn, passedWhileOn, sentAfterOnAck, onLostOnRestart>>

\* The DB write fails: set_air_gap returns Err and nothing changes.
ToggleWriteFails(t) ==
    /\ tpc[t] = "write"
    /\ tpc' = [tpc EXCEPT ![t] = "done"]
    /\ UNCHANGED <<disk, mem, tval, rpc, restarts, uiOn, passedWhileOn, sentAfterOnAck, onLostOnRestart>>

\* Then it updates memory and returns Ok, so the UI shows the new mode.
ToggleStore(t) ==
    /\ tpc[t] = "store"
    /\ mem' = tval[t]
    /\ uiOn' = tval[t]
    /\ tpc' = [tpc EXCEPT ![t] = "done"]
    /\ UNCHANGED <<disk, tval, rpc, restarts, passedWhileOn, sentAfterOnAck, onLostOnRestart>>

\* Scheme and host checks are pure and unit-tested, so only the mode matters here.
RequestCheck(r) ==
    /\ rpc[r] = "idle"
    /\ rpc' = [rpc EXCEPT ![r] = IF mem THEN "blocked" ELSE "checked"]
    /\ passedWhileOn' = (passedWhileOn \/ (mem /\ rpc'[r] = "checked"))
    /\ UNCHANGED <<disk, mem, tpc, tval, restarts, uiOn, sentAfterOnAck, onLostOnRestart>>

RequestSend(r) ==
    /\ rpc[r] = "checked"
    /\ rpc' = [rpc EXCEPT ![r] = "sent"]
    /\ sentAfterOnAck' = (sentAfterOnAck \/ UserExpectsOn)
    /\ UNCHANGED <<disk, mem, tpc, tval, restarts, uiOn, passedWhileOn, onLostOnRestart>>

\* Quit or crash at any point, then relaunch. In-flight work is gone; the DB row survives.
Restart ==
    /\ restarts < MaxRestarts
    /\ restarts' = restarts + 1
    /\ mem' = Load(disk)
    /\ uiOn' = Load(disk)
    /\ onLostOnRestart' = (onLostOnRestart \/ (UserExpectsOn /\ ~Load(disk)))
    /\ tpc' = [t \in Toggles |-> "idle"]
    /\ tval' \in [Toggles -> BOOLEAN]
    /\ rpc' = [r \in Requests |-> "idle"]
    /\ UNCHANGED <<disk, passedWhileOn, sentAfterOnAck>>

\* All calls finished; stutter so TLC does not report a deadlock.
Done ==
    /\ \A t \in Toggles : tpc[t] = "done"
    /\ \A r \in Requests : rpc[r] \in {"blocked", "sent"}
    /\ UNCHANGED vars

Next ==
    \/ \E t \in Toggles : ToggleStart(t) \/ ToggleWrite(t) \/ ToggleWriteFails(t) \/ ToggleStore(t)
    \/ \E r \in Requests : RequestCheck(r) \/ RequestSend(r)
    \/ Restart
    \/ Done

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* While Air-Gap is ON in the guard, no request passes the check.
CheckedOnlyWhenOff == ~passedWhileOn

\* Once the user is told Air-Gap is ON, no request leaves the app until they turn it OFF.
NoSendAfterOnAck == ~sentAfterOnAck

\* If the user was last told Air-Gap is ON, a relaunch keeps it ON.
OnSurvivesRestart == ~onLostOnRestart
=============================================================================
