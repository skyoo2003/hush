// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";
import { APP_NAME } from "./brand";
import { AboutPanel } from "./components/AboutPanel";
import { DataPanel } from "./components/DataPanel";
import { FolderAndSchedulePanel } from "./components/FolderAndSchedulePanel";
import { QuickSearchModal } from "./components/QuickSearchModal";
import { SecurityPanel } from "./components/SecurityPanel";
import { ShortcutSetting } from "./components/ShortcutSetting";
import { SponsorBanner } from "./components/SponsorBanner";
import { SponsorPanel } from "./components/SponsorPanel";
import { ThemeSetting } from "./components/ThemeSetting";

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
			<ThemeSetting />
			<DataPanel />
			<SponsorPanel />
			<AboutPanel />
			<SponsorBanner />
		</main>
	);
}
