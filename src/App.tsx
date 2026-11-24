// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";
import { APP_NAME } from "./brand";
import { AirGapToggle } from "./components/AirGapToggle";
import { FolderAndSchedulePanel } from "./components/FolderAndSchedulePanel";
import { QuickSearchModal } from "./components/QuickSearchModal";
import { ShortcutSetting } from "./components/ShortcutSetting";

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
			<AirGapToggle />
			<section aria-labelledby="sponsor-heading">
				<h2 id="sponsor-heading">Sponsor</h2>
				{/* shortcut: placeholder until the sponsor feature (badge, theme) is built. */}
				<p>Coming soon.</p>
			</section>
		</main>
	);
}
