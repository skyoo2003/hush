// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";
import { APP_NAME } from "./brand";
import { FolderAndSchedulePanel } from "./components/FolderAndSchedulePanel";
import { QuickSearchModal } from "./components/QuickSearchModal";
import { SecurityPanel } from "./components/SecurityPanel";
import { ShortcutSetting } from "./components/ShortcutSetting";
import { t } from "./i18n";

// Both windows load index.html; the window label picks the UI.
const label = getCurrentWindow().label;

export function App() {
	if (label === "quick-search") {
		return <QuickSearchModal />;
	}
	return (
		<main>
			<h1>{APP_NAME}</h1>
			<FolderAndSchedulePanel />
			<ShortcutSetting />
			<SecurityPanel />
			<section aria-labelledby="sponsor-heading">
				<h2 id="sponsor-heading">{t("main.sponsor")}</h2>
				{/* shortcut: placeholder until the sponsor feature (badge, theme) is built. */}
				<p>{t("main.sponsorSoon")}</p>
			</section>
		</main>
	);
}
