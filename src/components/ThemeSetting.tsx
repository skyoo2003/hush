// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import { Group, Row } from "../settings/Form";
import { Select } from "../ui/Select";
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
		<Group title={t("theme.heading")}>
			<Row label={t("theme.label")}>
				<Select
					label={t("theme.label")}
					value={settings?.theme ?? "system"}
					disabled={!settings}
					options={THEMES.map((v) => ({ value: v, label: t(`theme.${v}`) }))}
					onValueChange={(theme) =>
						invoke<Settings>("set_theme", { theme }).then(setSettings)
					}
				/>
			</Row>
			<Row label={t("accent.label")}>
				<Select
					label={t("accent.label")}
					value={settings?.accent ?? "system"}
					disabled={!settings}
					options={ACCENTS.map((v) => ({
						value: v,
						label: t(`accent.${v}`, { app: APP_NAME }),
					}))}
					onValueChange={(accent) =>
						invoke<Settings>("set_accent", { accent }).then(setSettings)
					}
				/>
			</Row>
		</Group>
	);
}
