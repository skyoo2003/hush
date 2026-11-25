// SPDX-License-Identifier: GPL-3.0-or-later
// Dev-only stand-in for the Rust backend: every command and event the frontend uses, answered from
// fixtures. Loaded only by the `mock-ipc` Vite plugin in `vite.config.ts` (dev server with
// VITE_MOCK_IPC=1); CI fails if MOCK_MARKER appears in dist/.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { About } from "../ipc/About";
import type { Accent } from "../ipc/Accent";
import type { AddedFolder } from "../ipc/AddedFolder";
import type { AppError } from "../ipc/AppError";
import type { Appearance } from "../ipc/Appearance";
import type { AvailableUpdate } from "../ipc/AvailableUpdate";
import type { Current } from "../ipc/Current";
import type { DownloadProgress } from "../ipc/DownloadProgress";
import type { ErrorCode } from "../ipc/ErrorCode";
import type { FailedDocument } from "../ipc/FailedDocument";
import type { FolderInfo } from "../ipc/FolderInfo";
import type { ImportedFile } from "../ipc/ImportedFile";
import type { IndexProgress } from "../ipc/IndexProgress";
import type { IndexStatus } from "../ipc/IndexStatus";
import type { LegalText } from "../ipc/LegalText";
import type { ModelInfo } from "../ipc/ModelInfo";
import type { OpenMode } from "../ipc/OpenMode";
import type { ScheduleSettings } from "../ipc/ScheduleSettings";
import type { ScheduleStatus } from "../ipc/ScheduleStatus";
import type { SearchResults } from "../ipc/SearchResults";
import type { Settings } from "../ipc/Settings";
import type { SponsorLink } from "../ipc/SponsorLink";
import type { Theme } from "../ipc/Theme";

export const MOCK_MARKER = "__LOCITE_MOCK_IPC__";

/** Every command the frontend invokes: arguments as the frontend sends them, results as Rust returns them. */
export type Commands = {
	search: (a: { query: string; utcOffsetMinutes: number }) => SearchResults;
	open_source: (a: { chunkId: number; mode: OpenMode }) => null;
	copy_citation: (a: { chunkId: number }) => null;
	hide_window: () => null;
	list_folders: () => FolderInfo[];
	add_watched_folder: () => AddedFolder | null;
	remove_watched_folder: (a: { id: number }) => null;
	set_folder_enabled: (a: { id: number; enabled: boolean }) => null;
	list_failed_documents: (a: { folderId: number }) => FailedDocument[];
	retry_document: (a: { docId: number }) => null;
	set_exclude_patterns: (a: { patterns: string }) => Settings;
	get_index_status: () => IndexStatus;
	get_schedule: () => ScheduleStatus;
	set_schedule: (a: { settings: ScheduleSettings }) => ScheduleStatus;
	get_settings: () => Settings;
	set_air_gap: (a: { enabled: boolean }) => null;
	set_shortcut: (a: { shortcut: string }) => Settings;
	set_supporter: (a: { on: boolean }) => Settings;
	set_theme: (a: { theme: Theme }) => Settings;
	set_accent: (a: { accent: Accent }) => Settings;
	get_appearance: () => Appearance;
	dismiss_sponsor_banner: () => Settings;
	open_sponsor: (a: { channel: number }) => SponsorLink;
	list_models: () => ModelInfo[];
	download_model: (a: { id: string; airGapOffForDownload: boolean }) => null;
	cancel_model_download: (a: { id: string }) => boolean;
	import_model_file: () => ImportedFile[] | null;
	check_for_update: () => AvailableUpdate | null;
	install_update: () => null;
	export_diagnostics: () => boolean;
	delete_all_indexes: () => null;
	get_about: () => About;
	read_legal_text: (a: { which: LegalText }) => string;
};
export type Command = keyof Commands;

/** Events Rust emits that the frontend listens to, with their payloads. */
export type Events = {
	"index://progress": IndexProgress;
	"index://schedule": Current;
	"models://progress": DownloadProgress;
	"sponsor://qr": SponsorLink;
	"ui://accent": string;
	"ui://appearance": Appearance;
};
export type MockEvent = {
	[E in keyof Events]: { event: E; payload: Events[E] };
}[keyof Events];

/** The backend state one scenario runs against. */
export type Fixture = {
	settings: Settings;
	/** NSColor.controlAccentColor as Rust reports it. */
	systemAccent: string | null;
	folders: FolderInfo[];
	failed: Record<number, FailedDocument[]>;
	index: IndexStatus;
	schedule: ScheduleStatus;
	models: ModelInfo[];
	about: About;
	legal: string;
	search: (query: string) => SearchResults;
	sponsorQr: SponsorLink;
	update: AvailableUpdate | null;
	/** What the folder picker "picks"; null is a cancelled picker. */
	addResult: AddedFolder | null;
	/** Commands that reject with this error code; "*" for every command. */
	fail: Partial<Record<Command | "*", ErrorCode>>;
	/** Commands that never answer, for loading states. */
	hang: Command[] | "all";
	/** Emitted once the UI has mounted, in order. */
	events: MockEvent[];
};

function handlers(f: Fixture): Commands {
	const settings = (patch: Partial<Settings>) => {
		Object.assign(f.settings, patch);
		return { ...f.settings };
	};
	const appearance = (): Appearance => ({
		theme: f.settings.theme,
		accent: f.settings.accent,
		systemAccent: f.systemAccent,
	});
	// As appearance.rs does: a theme or accent change reaches every window as an event.
	const appearanceChanged = (s: Settings) => {
		emit("ui://appearance", appearance());
		return s;
	};
	return {
		search: ({ query }) => f.search(query),
		open_source: () => null,
		copy_citation: () => null,
		hide_window: () => null,
		list_folders: () => f.folders.map((x) => ({ ...x })),
		add_watched_folder: () => {
			if (f.addResult) f.folders.push(f.addResult.folder);
			return f.addResult;
		},
		remove_watched_folder: ({ id }) => {
			f.folders = f.folders.filter((x) => x.id !== id);
			return null;
		},
		set_folder_enabled: ({ id, enabled }) => {
			for (const x of f.folders) if (x.id === id) x.enabled = enabled;
			return null;
		},
		list_failed_documents: ({ folderId }) => f.failed[folderId] ?? [],
		retry_document: () => null,
		set_exclude_patterns: ({ patterns }) =>
			settings({ excludePatterns: patterns }),
		get_index_status: () => f.index,
		get_schedule: () => f.schedule,
		set_schedule: ({ settings: s }) => {
			f.schedule = { ...f.schedule, settings: s };
			return f.schedule;
		},
		get_settings: () => ({ ...f.settings }),
		set_air_gap: ({ enabled }) => {
			settings({ airGap: enabled });
			return null;
		},
		set_shortcut: ({ shortcut }) =>
			settings({ shortcut, shortcutRegistered: true }),
		set_supporter: ({ on }) =>
			settings({
				supporter: on,
				sponsorBanner: on ? false : f.settings.sponsorBanner,
			}),
		set_theme: ({ theme }) => appearanceChanged(settings({ theme })),
		set_accent: ({ accent }) => appearanceChanged(settings({ accent })),
		get_appearance: appearance,
		dismiss_sponsor_banner: () => settings({ sponsorBanner: false }),
		open_sponsor: () => (f.settings.airGap ? f.sponsorQr : { kind: "browser" }),
		list_models: () => f.models.map((m) => ({ ...m })),
		download_model: () => null,
		cancel_model_download: () => true,
		import_model_file: () => null,
		check_for_update: () => f.update,
		install_update: () => null,
		export_diagnostics: () => true,
		delete_all_indexes: () => null,
		get_about: () => f.about,
		read_legal_text: () => f.legal,
	};
}

/** Installs the mock as the Tauri IPC of this page, as window `label`. Returns a function that
 * emits the fixture's events and resolves once every command so far has answered. */
export function installMockIpc(label: string, f: Fixture) {
	const h = handlers(f);
	let inFlight = 0;
	mockWindows(label);
	mockIPC(
		async (cmd, args) => {
			if (!(cmd in h)) throw new Error(`mockIpc: no handler for "${cmd}"`);
			const name = cmd as Command;
			if (f.hang === "all" || f.hang.includes(name))
				return new Promise(() => {});
			inFlight++;
			try {
				// One macrotask, as a real IPC round trip would take.
				await new Promise((r) => setTimeout(r, 0));
				const code = f.fail[name] ?? f.fail["*"];
				if (code) throw { code, message: `mock ${code}` } satisfies AppError;
				return (h[name] as (a: unknown) => unknown)(args);
			} finally {
				inFlight--;
			}
		},
		{ shouldMockEvents: true },
	);
	(window as unknown as Record<string, boolean>)[MOCK_MARKER] = true;
	return {
		emitEvents: async () => {
			for (const e of f.events) await emit(e.event, e.payload);
		},
		idle: () => inFlight === 0,
	};
}
