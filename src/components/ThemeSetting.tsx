// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import type { Accent } from "../ipc/Accent";
import type { Settings } from "../ipc/Settings";
import type { Theme } from "../ipc/Theme";

const THEMES: Theme[] = ["system", "light", "dark", "oled_dark", "terminal"];
const ACCENTS: Accent[] = ["system", "brand"];

// Every theme is open to everyone (absolute rule 8). Rust saves the choice and emits
// `ui://appearance`, which src/ui/appearance.ts applies in every window.
export function ThemeSetting() {
	const [settings, setSettings] = useState<Settings | null>(null);

	useEffect(() => {
		invoke<Settings>("get_settings").then(setSettings, () => {});
	}, []);

	return (
		<section aria-labelledby="theme-heading">
			<h2 id="theme-heading">{t("theme.heading")}</h2>
			<label>
				{t("theme.label")}{" "}
				<select
					value={settings?.theme ?? "system"}
					disabled={!settings}
					onChange={(e) =>
						invoke<Settings>("set_theme", {
							theme: e.target.value as Theme,
						}).then(setSettings)
					}
				>
					{THEMES.map((v) => (
						<option key={v} value={v}>
							{t(`theme.${v}`)}
						</option>
					))}
				</select>
			</label>{" "}
			<label>
				{t("accent.label")}{" "}
				<select
					value={settings?.accent ?? "system"}
					disabled={!settings}
					onChange={(e) =>
						invoke<Settings>("set_accent", {
							accent: e.target.value as Accent,
						}).then(setSettings)
					}
				>
					{ACCENTS.map((v) => (
						<option key={v} value={v}>
							{t(`accent.${v}`, { app: APP_NAME })}
						</option>
					))}
				</select>
			</label>
		</section>
	);
}
