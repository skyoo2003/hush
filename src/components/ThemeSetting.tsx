// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t } from "../i18n";
import type { Settings } from "../ipc/Settings";
import type { Theme } from "../ipc/Theme";

const THEMES: Theme[] = ["system", "oled_dark", "terminal"];

// Every theme is open to everyone (absolute rule 8); `theme.css` keys off `data-theme`.
export function ThemeSetting() {
	const [theme, setTheme] = useState<Theme | null>(null);

	useEffect(() => {
		if (theme) document.documentElement.dataset.theme = theme;
	}, [theme]);

	useEffect(() => {
		invoke<Settings>("get_settings").then(
			(s) => setTheme(s.theme),
			() => setTheme("system"),
		);
	}, []);

	return (
		<section aria-labelledby="theme-heading">
			<h2 id="theme-heading">{t("theme.heading")}</h2>
			<label>
				{t("theme.label")}{" "}
				<select
					value={theme ?? "system"}
					disabled={!theme}
					onChange={(e) =>
						invoke<Settings>("set_theme", {
							theme: e.target.value as Theme,
						}).then((s) => setTheme(s.theme))
					}
				>
					{THEMES.map((v) => (
						<option key={v} value={v}>
							{t(`theme.${v}`)}
						</option>
					))}
				</select>
			</label>
		</section>
	);
}
