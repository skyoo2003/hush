// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { type KeyboardEvent, useEffect, useState } from "react";
import type { AppError } from "../ipc/AppError";
import type { Settings } from "../ipc/Settings";
import { formatShortcut, shortcutFromKey } from "../shortcut";

function errorText(e: unknown, shortcut: string): string {
  switch ((e as Partial<AppError>)?.code) {
    case "shortcut_unavailable":
      return `${formatShortcut(shortcut)} is in use by another app. Try another one.`;
    case "shortcut_invalid":
      return `${formatShortcut(shortcut)} cannot be used as a shortcut. Try another one.`;
    default:
      return "Could not save the shortcut. It is unchanged.";
  }
}

export function ShortcutSetting() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Refetch on focus: the main window loads once and is shown and hidden after that.
    const refresh = () =>
      invoke<Settings>("get_settings").then(setSettings, () =>
        setError("Could not read the shortcut setting."),
      );
    void refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  const save = async (shortcut: string) => {
    setRecording(false);
    try {
      setSettings(await invoke<Settings>("set_shortcut", { shortcut }));
      setError(null);
    } catch (e) {
      setError(errorText(e, shortcut));
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording || e.key === "Tab") return;
    e.preventDefault();
    if (e.key === "Escape") {
      setRecording(false);
      return;
    }
    const shortcut = shortcutFromKey(e.nativeEvent);
    if (shortcut) void save(shortcut);
  };

  return (
    <section>
      <h2>Quick search shortcut</h2>
      {settings && !settings.shortcutRegistered && (
        <p role="alert">
          {formatShortcut(settings.shortcut)} is in use by another app, so quick
          search has no shortcut. Choose another one below.
        </p>
      )}
      <button
        type="button"
        aria-label="Quick search shortcut. Press to record a new one."
        disabled={!settings}
        onClick={() => setRecording(true)}
        onKeyDown={onKeyDown}
        onBlur={() => setRecording(false)}
      >
        {recording
          ? "Press the new shortcut, or Esc to cancel"
          : settings
            ? formatShortcut(settings.shortcut)
            : "…"}
      </button>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
