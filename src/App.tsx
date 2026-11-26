// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";
import { QuickSearchModal } from "./components/QuickSearchModal";
import { MainWindow } from "./library/MainWindow";
import { SettingsWindow } from "./settings/SettingsWindow";

// Every window loads index.html; the window label picks the UI.
const label = getCurrentWindow().label;

export function App() {
	if (label === "quick-search") return <QuickSearchModal />;
	if (label === "settings") return <SettingsWindow />;
	return <MainWindow />;
}
