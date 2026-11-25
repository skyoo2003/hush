// SPDX-License-Identifier: GPL-3.0-or-later
// Every screen × state the harness can show. A state is the backend fixture plus the user steps
// that lead to it; /__harness lists them and `pnpm ui:snap` photographs each one.
import { DOCUMENTS } from "./fixtures/documents";
import { models } from "./fixtures/state";
import type { Accent } from "../ipc/Accent";
import type { Theme } from "../ipc/Theme";
import type { Fixture } from "./mockIpc";

export const LANGS = ["en", "ko"] as const;
export type Lang = (typeof LANGS)[number];
export const THEMES = ["light", "dark"] as const;

/** Real window sizes from tauri.conf.json and desktop.rs. "tokens" is the harness's own page
 * (TokensPage.tsx), tall enough for every token and with no strings to translate. */
export const WINDOWS = {
	"quick-search": { label: "quick-search", width: 680, height: 460, langs: LANGS },
	main: { label: "main", width: 1080, height: 720, langs: LANGS },
	tokens: { label: "main", width: 1080, height: 1960, langs: ["en"] },
} as const;
export type Screen = keyof typeof WINDOWS;

type Text = string | Record<Lang, string>;
export type Step =
	| { fill: string; text: Text }
	| { click: string }
	| { select: string; value: string }
	| { key: string; meta?: boolean }
	| { scroll: string }
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

// Main window sections, in App.tsx order.
const section = (n: number) => `main > section:nth-of-type(${n})`;
const FOLDERS = section(1);
const SHORTCUT = section(2);
const SECURITY = section(3);
const THEME = section(4);
const DATA = section(5);
const SPONSOR = section(6);
const ABOUT = section(7);

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

	// Main window (one scrolling page of settings panels today)
	{
		screen: "main",
		state: "overview",
		note: "Top of the window: folders with counts, errors, a paused and an unavailable folder",
	},
	{
		screen: "main",
		state: "first-run",
		note: "No folders, no model, never indexed",
		setup: firstRun,
	},
	{
		screen: "main",
		state: "first-run-model",
		note: "First run scrolled to the model setup",
		setup: firstRun,
		steps: [{ scroll: SECURITY }],
	},
	{
		screen: "main",
		state: "indexing",
		note: "Indexing in progress with failures",
		setup: (f) => {
			f.index.progress = {
				queued: 340,
				running: 4,
				done: 1200,
				failed: 2,
				paused: false,
			};
			f.schedule.current = {
				state: "normal",
				reason: "default",
				plan: { parseWorkers: 4, embedding: true },
			};
		},
	},
	{
		screen: "main",
		state: "indexing-paused",
		note: "Indexing paused by the user",
		setup: (f) => {
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
		},
	},
	{
		screen: "main",
		state: "folder-errors",
		note: "A folder's failed files listed",
		steps: [
			{ click: `${FOLDERS} button[aria-expanded]` },
			{ wait: `${FOLDERS} ul ul` },
		],
	},
	{
		screen: "main",
		state: "folder-remove-confirm",
		note: "Remove folder confirmation",
		steps: [
			{ click: `${FOLDERS} li button:not([aria-expanded])` },
			{ wait: "fieldset" },
		],
	},
	{
		screen: "main",
		state: "folder-broad-path",
		note: "Added a very broad folder: warning",
		setup: (f) => {
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
		},
		steps: [{ click: `${FOLDERS} > button` }],
	},
	{
		screen: "main",
		state: "folder-add-error",
		note: "Adding an overlapping folder fails",
		setup: (f) => {
			f.fail.add_watched_folder = "folder_overlap";
		},
		steps: [{ click: `${FOLDERS} > button` }, { wait: "[role=alert]" }],
	},
	{
		screen: "main",
		state: "schedule",
		note: "Schedule with allowed hours",
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
		steps: [{ scroll: "#schedule-heading" }],
	},
	{
		screen: "main",
		state: "shortcut-conflict",
		note: "Another app holds the shortcut",
		setup: (f) => {
			f.settings.shortcutRegistered = false;
		},
		steps: [{ scroll: SHORTCUT }],
	},
	{
		screen: "main",
		state: "airgap-on",
		note: "Privacy: Air-Gap on, model installed",
		steps: [{ scroll: SECURITY }],
	},
	{
		screen: "main",
		state: "airgap-off",
		note: "Privacy: Air-Gap off",
		setup: airGapOff,
		steps: [{ scroll: SECURITY }],
	},
	{
		screen: "main",
		state: "model-downloading",
		note: "A model download at 40%",
		setup: (f) => {
			airGapOff(f);
			const m = f.models[1];
			if (m) m.downloading = true;
			f.events = [
				{
					event: "models://progress",
					payload: {
						id: "bge-small-en-v1.5",
						done: 53_671_377,
						total: 134_178_443,
					},
				},
			];
		},
		steps: [{ scroll: `${SECURITY} h3` }],
	},
	{
		screen: "main",
		state: "update-available",
		note: "Checked for updates: 0.2.0 available",
		setup: airGapOff,
		steps: [
			{ click: `${SECURITY} > button:nth-of-type(2)` },
			{ wait: `${SECURITY} > button:nth-of-type(3)` },
			{ scroll: `${SECURITY} > h3:last-of-type` },
		],
	},
	{
		screen: "main",
		state: "update-error",
		note: "Update check failed",
		setup: (f) => {
			airGapOff(f);
			f.fail.check_for_update = "update_failed";
		},
		steps: [
			{ click: `${SECURITY} > button:nth-of-type(2)` },
			{ scroll: `${SECURITY} > h3:last-of-type` },
		],
	},
	{
		screen: "main",
		state: "theme-and-data",
		note: "Theme and data panels",
		steps: [{ scroll: THEME }],
	},
	{
		screen: "main",
		state: "delete-confirm",
		note: "Delete all indexes: what goes, what stays",
		steps: [{ click: `${DATA} > button:nth-of-type(2)` }, { scroll: DATA }],
	},
	{
		screen: "main",
		state: "export-failed",
		note: "Export diagnostics failed",
		setup: (f) => {
			f.fail.export_diagnostics = "diagnostics_failed";
		},
		steps: [
			{ click: `${DATA} > button` },
			{ wait: `${DATA} [role=status]` },
			{ scroll: DATA },
		],
	},
	{
		screen: "main",
		state: "sponsor",
		note: "Sponsor panel",
		steps: [{ scroll: SPONSOR }],
	},
	{
		screen: "main",
		state: "sponsor-qr",
		note: "Sponsor link as a QR code (Air-Gap on)",
		steps: [
			{ click: `${SPONSOR} > button` },
			{ wait: "figure" },
			{ scroll: SPONSOR },
		],
	},
	{
		screen: "main",
		state: "supporter",
		note: "Supporter badge on",
		setup: (f) => {
			f.settings.supporter = true;
		},
		steps: [{ scroll: SPONSOR }],
	},
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
		state: "about",
		note: "About and license",
		steps: [{ scroll: ABOUT }],
	},
	{
		screen: "main",
		state: "about-license-open",
		note: "License text expanded",
		steps: [
			{ click: `${ABOUT} summary` },
			{ wait: ".legal-text:not(:empty)" },
			{ scroll: ABOUT },
		],
	},
	{
		screen: "main",
		state: "theme-oled-dark",
		note: "OLED dark theme picked",
		setup: (f) => {
			f.settings.theme = "oled_dark";
		},
	},
	{
		screen: "main",
		state: "theme-terminal",
		note: "Terminal theme picked",
		setup: (f) => {
			f.settings.theme = "terminal";
		},
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
];

export const scenario = (screen: string | null, state: string | null) =>
	SCENARIOS.find((s) => s.screen === screen && s.state === state);
