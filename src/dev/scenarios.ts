// SPDX-License-Identifier: GPL-3.0-or-later
// Every screen × state the harness can show. A state is the backend fixture plus the user steps
// that lead to it; /__harness lists them and `pnpm ui:snap` photographs each one.
import { DOCUMENTS } from "./fixtures/documents";
import { models } from "./fixtures/state";
import type { Accent } from "../ipc/Accent";
import type { MenuAction } from "../ipc/MenuAction";
import type { SettingsPane } from "../ipc/SettingsPane";
import type { Theme } from "../ipc/Theme";
import type { Fixture } from "./mockIpc";

export const LANGS = ["en", "ko"] as const;
export type Lang = (typeof LANGS)[number];
export const THEMES = ["light", "dark"] as const;

/** Real window sizes from tauri.conf.json and desktop.rs. "tokens" is the harness's own page
 * (TokensPage.tsx), tall enough for every token and with no strings to translate. "components" and
 * "components-overlays" are ComponentsPage.tsx: every component, and one open overlay per state. */
export const WINDOWS = {
	"quick-search": {
		label: "quick-search",
		width: 680,
		height: 460,
		langs: LANGS,
	},
	main: { label: "main", width: 1080, height: 720, langs: LANGS },
	settings: { label: "settings", width: 680, height: 560, langs: LANGS },
	tokens: { label: "main", width: 1080, height: 1960, langs: ["en"] },
	components: { label: "main", width: 1080, height: 3800, langs: LANGS },
	"components-overlays": {
		label: "main",
		width: 1080,
		height: 720,
		langs: LANGS,
	},
} as const;
export type Screen = keyof typeof WINDOWS;

type Text = string | Record<Lang, string>;
export type Step =
	| { fill: string; text: Text }
	| { click: string }
	| { select: string; value: string }
	| { key: string; meta?: boolean }
	| { scroll: string }
	| { focus: string }
	| { context: string }
	| { wait: string };

export type Scenario = {
	screen: Screen;
	state: string;
	note: string;
	setup?: (f: Fixture) => void;
	steps?: Step[];
};

const SEARCH = "input[type=search]";
const query = (text: Text): Step[] => [{ fill: SEARCH, text }];
const results: Step = { wait: "[role=listbox]" };
const BUDGET = { en: "budget", ko: "예산" };

// Main window: folder rows in the sidebar's Folders group (fixture order), the toolbar's "+".
const folderRow = (n: number) =>
	`.sidebar-section:last-child li:nth-child(${n}) .sidebar-item`;
const ADD_FOLDER = ".toolbar-trailing .icon-btn:last-child";
const SETTINGS_ADD = "#pane-folders .form-group:first-child .form-row:last-child .btn";

const firstRun = (f: Fixture) => {
	f.folders = [];
	f.failed = {};
	f.models = models(false);
	f.index = {
		indexing: false,
		progress: { queued: 0, running: 0, done: 0, failed: 0, paused: false },
	};
	f.schedule.current = null;
};
const airGapOff = (f: Fixture) => {
	f.settings.airGap = false;
};
const indexing = (f: Fixture) => {
	f.index.progress = {
		queued: 340,
		running: 2,
		done: 1200,
		failed: 2,
		paused: false,
	};
	f.schedule.current = {
		state: "normal",
		reason: "default",
		plan: { parseWorkers: 4, embedding: true },
	};
};
const paused = (f: Fixture) => {
	f.index.progress = {
		queued: 340,
		running: 0,
		done: 1200,
		failed: 0,
		paused: true,
	};
	f.schedule.settings.mode = "paused";
	f.schedule.current = {
		state: "paused",
		reason: "user_paused",
		plan: { parseWorkers: 0, embedding: false },
	};
};
const broadPath = (f: Fixture) => {
	const folder = {
		id: 5,
		path: "/Users/demo",
		label: "demo",
		status: "active" as const,
		enabled: true,
		documents: DOCUMENTS.length,
		failed: 0,
		skipped: 0,
	};
	f.addResult = { folder, warning: "broad_path" };
};

// A main-window state reached through the native menu bar, which sends `menu://action`.
const menu = (
	state: string,
	note: string,
	action: MenuAction,
	wait?: string,
): Scenario => ({
	screen: "main",
	state,
	note,
	setup: (f) => {
		f.events.push({ event: "menu://action", payload: action });
	},
	steps: wait ? [{ wait }] : [],
});

// A settings state: the pane is picked as Rust does, with `settings://pane`, before any step.
const pane = (
	id: SettingsPane,
	note: string,
	more: Partial<Pick<Scenario, "state" | "setup" | "steps">> = {},
): Scenario => ({
	screen: "settings",
	state: more.state ?? id,
	note,
	setup: (f) => {
		f.events.push({ event: "settings://pane", payload: id });
		more.setup?.(f);
	},
	steps: [{ wait: `#pane-${id}:not([hidden])` }, ...(more.steps ?? [])],
});

// Tokens page: one state per theme with each accent, the live switch, and Increase Contrast
// (`pnpm ui:snap` emulates prefers-contrast: more for states named "increase-contrast").
const tokens = (theme: Theme, accent: Accent, note: string): Scenario => ({
	screen: "tokens",
	state: `${theme}-${accent}`,
	note,
	setup: (f) => {
		f.settings.theme = theme;
		f.settings.accent = accent;
	},
});
// Components page: everything in one state; each overlay opened by its trigger (ids in ComponentsPage).
const overlay = (
	state: string,
	note: string,
	open: Step[],
	wait: string,
): Scenario => ({
	screen: "components-overlays",
	state,
	note,
	steps: [...open, { wait }],
});
const keyOpen = (id: string): Step[] => [{ focus: id }, { key: "Enter" }];

export const SCENARIOS: Scenario[] = [
	// Quick search
	{ screen: "quick-search", state: "empty", note: "Just opened: hint only" },
	{
		screen: "quick-search",
		state: "loading",
		note: "Query typed, search still running",
		setup: (f) => {
			f.hang = ["search"];
		},
		steps: query(BUDGET),
	},
	{
		screen: "quick-search",
		state: "results",
		note: "Hits with highlights; first selected",
		steps: [...query(BUDGET), results],
	},
	{
		screen: "quick-search",
		state: "results-mixed",
		note: "Korean and English hits, long file name",
		steps: [...query("budget 예산"), results],
	},
	{
		screen: "quick-search",
		state: "keyboard-nav",
		note: "Arrow Down twice: selection on the third hit",
		steps: [
			...query(BUDGET),
			results,
			{ key: "ArrowDown" },
			{ key: "ArrowDown" },
		],
	},
	{
		screen: "quick-search",
		state: "filters",
		note: "Operators parsed into chips",
		steps: [...query("budget type:pdf after:2026-01"), results],
	},
	{
		screen: "quick-search",
		state: "partial-indexing",
		note: "Results while 342 files are still indexing",
		setup: (f) => {
			f.index.progress = {
				queued: 340,
				running: 2,
				done: 1200,
				failed: 0,
				paused: false,
			};
		},
		steps: [...query(BUDGET), results],
	},
	{
		screen: "quick-search",
		state: "no-results",
		note: "Nothing matches",
		steps: [
			...query({ en: "zebra crossing", ko: "얼룩말" }),
			{ wait: "main > p:not([role])" },
		],
	},
	{
		screen: "quick-search",
		state: "model-missing",
		note: "Search refused: no embedding model",
		setup: (f) => {
			f.fail.search = "model_missing";
		},
		steps: [...query(BUDGET), { wait: "[role=alert]" }],
	},
	{
		screen: "quick-search",
		state: "error",
		note: "Search failed",
		setup: (f) => {
			f.fail.search = "search_failed";
		},
		steps: [...query(BUDGET), { wait: "[role=alert]" }],
	},
	{
		screen: "quick-search",
		state: "source-missing",
		note: "Enter on a hit whose file was moved",
		setup: (f) => {
			f.fail.open_source = "source_not_found";
		},
		steps: [
			...query(BUDGET),
			results,
			{ key: "Enter" },
			{ wait: "[role=alert]" },
		],
	},
	{
		screen: "quick-search",
		state: "copied",
		note: "⌘C copied the citation",
		steps: [...query(BUDGET), results, { key: "c", meta: true }],
	},

	// Main window: the library (docs/design/ia.md)
	{ screen: "main", state: "search-empty", note: "Opens on Search: hint only" },
	{
		screen: "main",
		state: "search-results",
		note: "Toolbar search: hits, first selected",
		steps: [...query(BUDGET), results],
	},
	{
		screen: "main",
		state: "search-keyboard",
		note: "Arrow Down twice from the toolbar field",
		steps: [...query(BUDGET), results, { key: "ArrowDown" }, { key: "ArrowDown" }],
	},
	{
		screen: "main",
		state: "search-filters",
		note: "Operators parsed into tags",
		steps: [...query("budget type:pdf after:2026-01"), results],
	},
	{
		screen: "main",
		state: "search-partial-indexing",
		note: "Results while 342 files are still indexing",
		setup: indexing,
		steps: [...query(BUDGET), results],
	},
	{
		screen: "main",
		state: "search-no-results",
		note: "Nothing matches",
		steps: [
			...query({ en: "zebra crossing", ko: "얼룩말" }),
			{ wait: ".library-search .empty-state" },
		],
	},
	{
		screen: "main",
		state: "search-model-missing",
		note: "Search refused: no embedding model",
		setup: (f) => {
			f.fail.search = "model_missing";
		},
		steps: [...query(BUDGET), { wait: "[role=alert]" }],
	},
	{
		screen: "main",
		state: "search-copied",
		note: "⌘C on the selected hit copied its citation",
		steps: [...query(BUDGET), results, { key: "c", meta: true }],
	},
	menu("ask", "Ask: not in this version yet", { kind: "navigate", section: "ask" }),
	menu("compare", "Compare: not in this version yet", {
		kind: "navigate",
		section: "compare",
	}),
	menu("changes", "Changes: not in this version yet", {
		kind: "navigate",
		section: "changes",
	}),
	{
		screen: "main",
		state: "folder",
		note: "A folder with two failed files",
		steps: [{ click: folderRow(1) }, { wait: ".folder-failed" }],
	},
	{
		screen: "main",
		state: "folder-paused",
		note: "A paused folder, left out of search",
		steps: [{ click: folderRow(3) }, { wait: ".folder-page" }],
	},
	{
		screen: "main",
		state: "folder-unavailable",
		note: "A folder on a disconnected volume",
		steps: [{ click: folderRow(4) }, { wait: ".folder-page" }],
	},
	{
		screen: "main",
		state: "first-run",
		note: "No folders, never indexed",
		setup: firstRun,
	},
	{
		screen: "main",
		state: "indexing",
		note: "Sidebar footer while indexing, details open",
		setup: indexing,
		steps: [{ click: ".index-footer" }, { wait: ".index-footer-details" }],
	},
	{
		screen: "main",
		state: "indexing-paused",
		note: "Indexing paused by the user",
		setup: paused,
		steps: [{ click: ".index-footer" }, { wait: ".index-footer-details" }],
	},
	{
		screen: "main",
		state: "airgap-off",
		note: "Toolbar indicator with Air-Gap off",
		setup: airGapOff,
	},
	{
		screen: "main",
		state: "add-folder-broad-path",
		note: "Added a very broad folder: notice, folder page",
		setup: broadPath,
		steps: [{ click: ADD_FOLDER }, { wait: ".folder-page" }],
	},
	{
		screen: "main",
		state: "add-folder-error",
		note: "Adding an overlapping folder fails",
		setup: (f) => {
			f.fail.add_watched_folder = "folder_overlap";
		},
		steps: [{ click: ADD_FOLDER }, { wait: "[role=alert]" }],
	},
	menu("shortcuts", "Help › Keyboard Shortcuts sheet", { kind: "show_shortcuts" }, "[role=dialog]"),
	menu("sidebar-hidden", "View › Hide Sidebar", { kind: "toggle_sidebar" }, ".library[data-sidebar=hidden]"),
	{
		screen: "main",
		state: "sponsor-banner",
		note: "One-time milestone banner",
		setup: (f) => {
			f.settings.sponsorBanner = true;
		},
		steps: [{ wait: ".sponsor-banner" }],
	},
	{
		screen: "main",
		state: "theme-oled-dark",
		note: "OLED dark theme picked",
		setup: (f) => {
			f.settings.theme = "oled_dark";
		},
		steps: [...query(BUDGET), results],
	},
	{
		screen: "main",
		state: "theme-terminal",
		note: "Terminal theme picked",
		setup: (f) => {
			f.settings.theme = "terminal";
		},
		steps: [...query(BUDGET), results],
	},
	{
		screen: "main",
		state: "loading",
		note: "Backend has not answered yet",
		setup: (f) => {
			f.hang = "all";
		},
	},
	{
		screen: "main",
		state: "backend-error",
		note: "Every command fails",
		setup: (f) => {
			f.fail["*"] = "internal";
		},
	},

	// Settings window: one state per tab, then the states each tab holds
	pane("general", "General: software update, data"),
	pane("general", "Update check: 0.2.0 available", {
		state: "general-update-available",
		setup: airGapOff,
		steps: [
			{ click: "#pane-general .btn" },
			{ wait: "#pane-general .btn-primary" },
		],
	}),
	pane("general", "Update check failed", {
		state: "general-update-error",
		setup: (f) => {
			airGapOff(f);
			f.fail.check_for_update = "update_failed";
		},
		steps: [{ click: "#pane-general .btn" }],
	}),
	pane("general", "Delete all indexes: destructive alert", {
		state: "general-delete-confirm",
		steps: [{ click: "#pane-general .btn-destructive" }, { wait: "[role=alertdialog]" }],
	}),
	pane("general", "Export diagnostics failed", {
		state: "general-export-failed",
		setup: (f) => {
			f.fail.export_diagnostics = "internal";
		},
		steps: [
			{ click: "#pane-general .form-group:nth-child(2) .btn" },
			{ wait: "#pane-general .form-group:nth-child(2) [role=status]" },
		],
	}),
	pane("folders", "Folders & Indexing: active, paused, unavailable folders"),
	pane("folders", "A folder's failed files listed", {
		state: "folders-errors",
		steps: [
			{ click: "#pane-folders button[aria-expanded]" },
			{ wait: "#pane-folders .form-row-note .form-list" },
		],
	}),
	pane("folders", "Remove folder: destructive alert", {
		state: "folders-remove-confirm",
		steps: [
			{ click: "#pane-folders .form-row-control > .btn:last-child" },
			{ wait: "[role=alertdialog]" },
		],
	}),
	pane("folders", "Added a very broad folder: warning", {
		state: "folders-broad-path",
		setup: broadPath,
		steps: [{ click: SETTINGS_ADD }, { wait: "#pane-folders .banner-info" }],
	}),
	pane("folders", "Adding an overlapping folder fails", {
		state: "folders-add-error",
		setup: (f) => {
			f.fail.add_watched_folder = "folder_overlap";
		},
		steps: [{ click: SETTINGS_ADD }, { wait: "#pane-folders [role=alert]" }],
	}),
	pane("folders", "Schedule with allowed hours", {
		state: "folders-schedule",
		setup: (f) => {
			f.schedule.settings = {
				mode: "auto",
				allowedHours: { startMinute: 22 * 60, endMinute: 7 * 60 },
				lowPower: "one_worker",
			};
			f.schedule.current = {
				state: "low_power",
				reason: "outside_hours",
				plan: { parseWorkers: 1, embedding: false },
			};
		},
		steps: [{ scroll: "#schedule-status" }],
	}),
	pane("folders", "No folders yet", { state: "folders-first-run", setup: firstRun }),
	pane("models", "Models: one installed, one not"),
	pane("models", "First run: no embedding model", {
		state: "models-first-run",
		setup: firstRun,
	}),
	pane("models", "A model download at 40%", {
		state: "models-downloading",
		setup: (f) => {
			airGapOff(f);
			const m = f.models[1];
			if (m) m.downloading = true;
			f.events.push({
				event: "models://progress",
				payload: {
					id: "bge-small-en-v1.5",
					done: 53_671_377,
					total: 134_178_443,
				},
			});
		},
	}),
	pane("privacy", "Privacy & Air-Gap: on"),
	pane("privacy", "Air-Gap off", { state: "privacy-airgap-off", setup: airGapOff }),
	pane("shortcuts", "Shortcuts: recorder and the shortcut list"),
	pane("shortcuts", "Another app holds the quick-search shortcut", {
		state: "shortcuts-conflict",
		setup: (f) => {
			f.settings.shortcutRegistered = false;
		},
	}),
	pane("appearance", "Appearance: theme and accent"),
	pane("sponsor", "Sponsor: channels and the thank-you badge switch"),
	pane("sponsor", "A tray sponsor click with Air-Gap on: QR", {
		state: "sponsor-qr",
		setup: (f) => {
			f.events.push({ event: "sponsor://qr", payload: f.sponsorQr });
		},
		steps: [{ wait: "#pane-sponsor figure" }],
	}),
	pane("sponsor", "Supporter badge on", {
		state: "sponsor-supporter",
		setup: (f) => {
			f.settings.supporter = true;
		},
	}),
	pane("about", "About and license"),
	pane("about", "License text expanded", {
		state: "about-license-open",
		steps: [
			{ click: "#pane-about summary" },
			{ wait: ".legal-text:not(:empty)" },
		],
	}),
	pane("general", "Backend has not answered yet", {
		state: "loading",
		setup: (f) => {
			f.hang = "all";
		},
	}),
	pane("folders", "Every command fails", {
		state: "backend-error",
		setup: (f) => {
			f.fail["*"] = "internal";
		},
	}),
	tokens("system", "system", "Follows the system scheme, macOS Blue accent"),
	tokens("system", "brand", "Follows the system scheme, brand indigo"),
	tokens("light", "brand", "Light"),
	tokens("dark", "brand", "Dark"),
	tokens("oled_dark", "brand", "OLED dark"),
	tokens("terminal", "brand", "Terminal"),
	{
		...tokens("system", "brand", ""),
		state: "switched",
		note: "System/brand, then Dark, System accent, and a system change to Pink",
		steps: [
			{ select: "#pick-theme", value: "dark" },
			{ select: "#pick-accent", value: "system" },
			{ select: "#pick-system-accent", value: "pink" },
		],
	},
	{
		...tokens("system", "system", ""),
		state: "increase-contrast",
		note: "prefers-contrast: more",
	},
	{
		screen: "components",
		state: "all",
		note: "Every component × state, light and dark; Smart Folders collapsed",
		steps: [
			{
				click:
					".cp-sidebar .sidebar-section:last-child .sidebar-section-trigger",
			},
			{ wait: ".cp-sidebar [data-state=closed]" },
		],
	},
	overlay(
		"popover",
		"Popover with arrow",
		[{ click: "#ov-popover" }],
		"[role=dialog]",
	),
	overlay(
		"menu",
		"Dropdown menu, opened by keyboard",
		keyOpen("#ov-menu"),
		"[role=menu]",
	),
	overlay(
		"context-menu",
		"Context menu on a right-click",
		[{ context: "#ov-context" }],
		"[role=menu]",
	),
	overlay(
		"select",
		"Pop-up button menu",
		keyOpen("#ov-select"),
		"[role=listbox]",
	),
	overlay(
		"tooltip",
		"Source chip excerpt on focus",
		[{ focus: "#ov-tooltip" }],
		"[role=tooltip]",
	),
	overlay(
		"sheet",
		"Sheet under the toolbar",
		[{ click: "#ov-sheet" }],
		"[role=dialog]",
	),
	overlay(
		"alert",
		"Alert, Return confirms",
		[{ click: "#ov-alert" }],
		"[role=alertdialog]",
	),
	overlay(
		"alert-destructive",
		"Destructive alert, focus starts on Cancel",
		[{ click: "#ov-alert-destructive" }],
		"[role=alertdialog]",
	),
	overlay("toast", "Toast with an action", [{ click: "#ov-toast" }], ".toast"),
];

export const scenario = (screen: string | null, state: string | null) =>
	SCENARIOS.find((s) => s.screen === screen && s.state === state);
