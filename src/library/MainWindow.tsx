// SPDX-License-Identifier: GPL-3.0-or-later
// The main window: the library. Sidebar (sections, folders, indexing status), toolbar (history,
// search, Air-Gap, add folder), and the current route's page. See docs/design/ia.md.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	ChevronLeft,
	ChevronRight,
	Columns2,
	Folder,
	FolderX,
	History as HistoryIcon,
	MessageSquareText,
	PauseCircle,
	Plus,
	Search,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { APP_NAME } from "../brand";
import { SponsorBanner } from "../components/SponsorBanner";
import { t } from "../i18n";
import type { AddedFolder } from "../ipc/AddedFolder";
import type { AppError } from "../ipc/AppError";
import type { FolderInfo } from "../ipc/FolderInfo";
import type { IndexProgress } from "../ipc/IndexProgress";
import type { IndexStatus } from "../ipc/IndexStatus";
import type { MenuAction } from "../ipc/MenuAction";
import type { MenuContext } from "../ipc/MenuContext";
import type { Settings } from "../ipc/Settings";
import { formatShortcut } from "../shortcut";
import { AirGapIndicator } from "../ui/AirGapIndicator";
import { Banner } from "../ui/Banner";
import { IconButton } from "../ui/IconButton";
import { SearchField } from "../ui/SearchField";
import { Sheet } from "../ui/Sheet";
import { Sidebar, type SidebarItem } from "../ui/Sidebar";
import { SplitView } from "../ui/SplitView";
import { Toolbar } from "../ui/Toolbar";
import { FolderPage } from "./FolderPage";
import {
	current,
	forgetFolder,
	go,
	type History,
	push,
	type Route,
	routeFromKey,
	routeKey,
	section,
	START,
} from "./history";
import { IndexFooter } from "./IndexFooter";
import { PlannedPage } from "./PlannedPage";
import { SearchPage } from "./SearchPage";
import { ShortcutTable } from "./ShortcutTable";
import { useLibrarySearch } from "./useLibrarySearch";
import { useStoredNumber } from "./useStoredNumber";
import "./library.css";

// Must match `ingest::queue::PROGRESS_EVENT`, `desktop::menu::MENU_EVENT`, `desktop::AIR_GAP_EVENT`.
const PROGRESS_EVENT = "index://progress";
const MENU_EVENT = "menu://action";
const AIR_GAP_EVENT = "settings://air-gap";

const SECTIONS: SidebarItem[] = [
	{ id: "search", label: t("nav.search"), icon: Search },
	{ id: "ask", label: t("nav.ask"), icon: MessageSquareText },
	{ id: "compare", label: t("nav.compare"), icon: Columns2 },
	{ id: "changes", label: t("nav.changes"), icon: HistoryIcon },
];

const folderItem = (f: FolderInfo): SidebarItem => ({
	id: routeKey({ kind: "folder", id: f.id }),
	label: f.label,
	// Shape, not color, tells the states apart; the folder page says it in words.
	icon:
		f.status === "unavailable" ? FolderX : f.enabled ? Folder : PauseCircle,
	count: f.documents,
});

function addError(e: unknown): string {
	switch ((e as Partial<AppError>)?.code) {
		case "folder_overlap":
			return t("folders.error.overlap");
		case "folder_unreadable":
			return t("folders.error.unreadable", { app: APP_NAME });
		case "model_missing":
			return t("folders.error.modelMissing");
		default:
			return t("error.generic");
	}
}

export function MainWindow() {
	const [history, setHistory] = useState<History>(START);
	const route = current(history);
	const navigate = useCallback((r: Route) => setHistory((h) => push(h, r)), []);
	const [folders, setFolders] = useState<FolderInfo[]>([]);
	const [status, setStatus] = useState<IndexStatus | null>(null);
	const [settings, setSettings] = useState<Settings | null>(null);
	const [sidebarWidth, setSidebarWidth] = useStoredNumber(
		"sidebar-width:main",
		220,
	);
	const [sidebarVisible, setSidebarVisible] = useState(true);
	const [query, setQuery] = useState("");
	const search = useLibrarySearch(query);
	const [shortcutsOpen, setShortcutsOpen] = useState(false);
	const [notice, setNotice] = useState<{
		tone: "info" | "error";
		text: string;
	} | null>(null);
	const field = useRef<HTMLInputElement>(null);

	const loadFolders = useCallback(() => {
		invoke<FolderInfo[]>("list_folders").then(setFolders, () =>
			setNotice({ tone: "error", text: t("folders.listFailed") }),
		);
	}, []);
	const loadSettings = useCallback(() => {
		invoke<Settings>("get_settings").then(setSettings, () => {});
	}, []);

	// Settings is another window: re-read what it may have changed when this one comes back.
	useEffect(() => {
		loadFolders();
		loadSettings();
		invoke<IndexStatus>("get_index_status").then(setStatus, () => {});
		const onFocus = () => {
			loadFolders();
			loadSettings();
		};
		window.addEventListener("focus", onFocus);
		const progress = listen<IndexProgress>(PROGRESS_EVENT, (e) => {
			setStatus({ indexing: true, progress: e.payload });
			loadFolders();
		});
		const airGap = listen<boolean>(AIR_GAP_EVENT, (e) =>
			setSettings((s) => (s ? { ...s, airGap: e.payload } : s)),
		);
		return () => {
			window.removeEventListener("focus", onFocus);
			void progress.then((f) => f());
			void airGap.then((f) => f());
		};
	}, [loadFolders, loadSettings]);

	// A folder removed in Settings leaves history too.
	useEffect(() => {
		const ids = new Set(folders.map((f) => f.id));
		setHistory((h) =>
			h.stack.reduce(
				(acc, r) =>
					r.kind === "folder" && !ids.has(r.id) ? forgetFolder(acc, r.id) : acc,
				h,
			),
		);
	}, [folders]);

	const addFolder = useCallback(async () => {
		setNotice(null);
		try {
			const added = await invoke<AddedFolder | null>("add_watched_folder");
			if (!added) return;
			if (added.warning === "broad_path")
				setNotice({ tone: "info", text: t("folders.broadPath") });
			navigate({ kind: "folder", id: added.folder.id });
		} catch (e) {
			setNotice({ tone: "error", text: addError(e) });
		} finally {
			loadFolders();
		}
	}, [loadFolders, navigate]);

	const focusSearch = useCallback(() => {
		field.current?.focus();
		field.current?.select();
	}, []);

	// Native menu bar → this window.
	const searchRef = useRef(search);
	searchRef.current = search;
	useEffect(() => {
		const unlisten = listen<MenuAction>(MENU_EVENT, ({ payload: a }) => {
			switch (a.kind) {
				case "navigate":
					if (a.section !== "folder") navigate({ kind: a.section });
					break;
				case "toggle_sidebar":
					setSidebarVisible((v) => !v);
					break;
				case "find":
					navigate({ kind: "search" });
					focusSearch();
					break;
				case "add_folder":
					void addFolder();
					break;
				case "copy_citation": {
					const s = searchRef.current;
					if (s.current) void s.act("copy", s.current);
					break;
				}
				case "show_shortcuts":
					setShortcutsOpen(true);
					break;
			}
		});
		return () => {
			void unlisten.then((f) => f());
		};
	}, [navigate, focusSearch, addFolder]);

	// This window → the menu bar's enabled items.
	const hasSelectedHit = route.kind === "search" && search.current !== undefined;
	useEffect(() => {
		const context: MenuContext = {
			section: section(route),
			folderId: route.kind === "folder" ? route.id : null,
			hasSelectedHit,
			sidebarVisible,
		};
		invoke("set_menu_context", { context }).catch(() => {});
	}, [route, hasSelectedHit, sidebarVisible]);

	// ⌘[ and ⌘] as in Finder; the menu bar has no Go menu.
	useEffect(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!e.metaKey || (e.key !== "[" && e.key !== "]")) return;
			e.preventDefault();
			setHistory((h) => go(h, e.key === "[" ? -1 : 1));
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, []);

	const pending = status ? status.progress.queued + status.progress.running : 0;
	const folder =
		route.kind === "folder" ? folders.find((f) => f.id === route.id) : undefined;

	const sidebar = (
		<div className="library-sidebar">
			<Sidebar
				label={t("nav.label")}
				selectedId={routeKey(route)}
				onSelect={(id) => {
					const r = routeFromKey(id);
					if (r) navigate(r);
				}}
				sections={[
					{ id: "library", items: SECTIONS },
					{
						id: "folders",
						title: t("nav.folders"),
						collapsible: true,
						items: folders.map(folderItem),
					},
				]}
			/>
			<IndexFooter status={status} />
		</div>
	);

	const content = (
		<div className="library-main">
			<Toolbar
				label={t("toolbar.label")}
				inset={!sidebarVisible}
				leading={
					<>
						<IconButton
							label={t("toolbar.back")}
							icon={ChevronLeft}
							disabled={history.index === 0}
							onClick={() => setHistory((h) => go(h, -1))}
						/>
						<IconButton
							label={t("toolbar.forward")}
							icon={ChevronRight}
							disabled={history.index === history.stack.length - 1}
							onClick={() => setHistory((h) => go(h, 1))}
						/>
					</>
				}
				center={
					<SearchField
						ref={field}
						className="library-search-field"
						role="combobox"
						aria-label={t("search.label")}
						aria-expanded={search.hits.length > 0 && route.kind === "search"}
						aria-controls="results"
						aria-activedescendant={
							route.kind === "search" && search.current
								? `hit-${search.selected}`
								: undefined
						}
						placeholder={t("search.placeholder")}
						clearLabel={t("ui.clear")}
						value={query}
						onChange={(v) => {
							setQuery(v);
							if (route.kind !== "search") navigate({ kind: "search" });
						}}
						onKeyDown={route.kind === "search" ? search.onKeyDown : undefined}
					/>
				}
				trailing={
					<>
						<AirGapIndicator
							on={settings?.airGap ?? true}
							label={settings?.airGap === false ? t("airgap.off") : t("airgap.on")}
							onClick={() => void invoke("open_settings", { pane: "privacy" })}
						/>
						<IconButton
							label={t("toolbar.addFolder")}
							icon={Plus}
							onClick={() => void addFolder()}
						/>
					</>
				}
			/>
			<main className="library-content">
				{notice && (
					<Banner
						tone={notice.tone}
						onDismiss={() => setNotice(null)}
						dismissLabel={t("ui.dismiss")}
					>
						{notice.text}
					</Banner>
				)}
				{route.kind === "search" && (
					<SearchPage query={query} search={search} pending={pending} />
				)}
				{(route.kind === "ask" ||
					route.kind === "compare" ||
					route.kind === "changes") && <PlannedPage kind={route.kind} />}
				{route.kind === "folder" && (
					<FolderPage
						folder={folder}
						onChanged={loadFolders}
						onSearchIn={(f) => {
							setQuery(`folder:${f.label} `);
							navigate({ kind: "search" });
							focusSearch();
						}}
					/>
				)}
			</main>
			<SponsorBanner />
		</div>
	);

	return (
		<div className="library" data-sidebar={sidebarVisible ? "shown" : "hidden"}>
			{sidebarVisible ? (
				<SplitView
					size={sidebarWidth}
					onSizeChange={setSidebarWidth}
					dividerLabel={t("ui.resize")}
					leading={sidebar}
					trailing={content}
				/>
			) : (
				content
			)}
			<Sheet
				open={shortcutsOpen}
				onOpenChange={setShortcutsOpen}
				title={t("shortcuts.title")}
			>
				<ShortcutTable
					quickSearch={
						settings?.shortcutRegistered ? formatShortcut(settings.shortcut) : null
					}
				/>
			</Sheet>
		</div>
	);
}
