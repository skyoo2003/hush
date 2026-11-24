// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { type KeyboardEvent, useEffect, useState } from "react";
import { t } from "../i18n";
import type { AppError } from "../ipc/AppError";
import type { Settings } from "../ipc/Settings";
import { formatShortcut, shortcutFromKey } from "../shortcut";

function errorText(e: unknown, shortcut: string): string {
  const keys = formatShortcut(shortcut);
  switch ((e as Partial<AppError>)?.code) {
    case "shortcut_unavailable":
      return t("shortcut.inUse", { keys });
    case "shortcut_invalid":
      return t("shortcut.invalid", { keys });
    default:
      return t("shortcut.saveFailed");
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
        setError(t("shortcut.readFailed")),
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
      <h2>{t("shortcut.heading")}</h2>
      {settings && !settings.shortcutRegistered && (
        <p role="alert">
          {t("shortcut.unregistered", {
            keys: formatShortcut(settings.shortcut),
          })}
        </p>
      )}
      <button
        type="button"
        aria-label={t("shortcut.buttonLabel")}
        disabled={!settings}
        onClick={() => setRecording(true)}
        onKeyDown={onKeyDown}
        onBlur={() => setRecording(false)}
      >
        {recording
          ? t("shortcut.recording")
          : settings
            ? formatShortcut(settings.shortcut)
            : "…"}
      </button>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
