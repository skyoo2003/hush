// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Settings } from "../ipc/Settings";

export function AirGapToggle() {
  // null until get_settings answers; the switch stays disabled until then.
  const [on, setOn] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Settings>("get_settings").then(
      (s) => setOn(s.airGap),
      () => setError("Could not read the Air-Gap setting."),
    );
  }, []);

  const toggle = async (next: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("set_air_gap", { enabled: next });
      setOn(next);
    } catch {
      setError("Could not save the Air-Gap setting. It is unchanged.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <label>
        <input
          type="checkbox"
          role="switch"
          aria-checked={on ?? true}
          checked={on ?? true}
          disabled={on === null || busy}
          onChange={(e) => void toggle(e.target.checked)}
        />
        Air-Gap mode
      </label>
      <p>
        {on === false
          ? "Off. Only model downloads and update checks may reach the network."
          : "On. Nothing leaves this computer."}
      </p>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
