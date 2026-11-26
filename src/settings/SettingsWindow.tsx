// SPDX-License-Identifier: GPL-3.0-or-later
// The settings window (⌘,): toolbar tabs, one pane shown at a time. Every pane stays mounted, so
// events (a tray QR, model progress) reach it and a pane keeps its state across tab switches.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	Cpu,
	FolderCog,
	Heart,
	Info,
	Keyboard,
	type LucideIcon,
	Palette,
	Settings2,
	ShieldCheck,
} from "lucide-react";
import { useEffect, useState } from "react";
import { AboutPanel } from "../components/AboutPanel";
import { DataPanel } from "../components/DataPanel";
import { FolderAndSchedulePanel } from "../components/FolderAndSchedulePanel";
import {
	type AirGap,
	AirGapPanel,
	ModelsPanel,
	UpdatePanel,
} from "../components/SecurityPanel";
import { ShortcutSetting } from "../components/ShortcutSetting";
import { type Qr, SponsorPanel } from "../components/SponsorPanel";
import { ThemeSetting } from "../components/ThemeSetting";
import { type MessageKey, t } from "../i18n";
import type { Settings } from "../ipc/Settings";
import type { SettingsPane } from "../ipc/SettingsPane";
import type { SponsorLink } from "../ipc/SponsorLink";
import { formatShortcut } from "../shortcut";
import { ShortcutTable } from "../library/ShortcutTable";
import { Group } from "./Form";
import "./settings.css";

// Must match `desktop::SETTINGS_PANE_EVENT`, `desktop::AIR_GAP_EVENT`, `sponsor::QR_EVENT`.
const PANE_EVENT = "settings://pane";
const AIR_GAP_EVENT = "settings://air-gap";
const QR_EVENT = "sponsor://qr";
const LAST_PANE = "settings-pane";

const TABS: readonly [SettingsPane, MessageKey, LucideIcon][] = [
	["general", "settings.general", Settings2],
	["folders", "settings.folders", FolderCog],
	["models", "settings.models", Cpu],
	["privacy", "settings.privacy", ShieldCheck],
	["shortcuts", "settings.shortcuts", Keyboard],
	["appearance", "settings.appearance", Palette],
	["sponsor", "settings.sponsor", Heart],
	["about", "settings.about", Info],
];

const isPane = (v: unknown): v is SettingsPane =>
	TABS.some(([p]) => p === v);

function useAirGap(): AirGap & { shortcut: string | null } {
	const [settings, setSettings] = useState<Settings | null>(null);
	const [error, setError] = useState<string | null>(null);
	useEffect(() => {
		const load = () =>
			invoke<Settings>("get_settings").then(setSettings, () =>
				setError(t("security.readFailed")),
			);
		void load();
		window.addEventListener("focus", load);
		const unlisten = listen<boolean>(AIR_GAP_EVENT, (e) =>
			setSettings((s) => (s ? { ...s, airGap: e.payload } : s)),
		);
		return () => {
			window.removeEventListener("focus", load);
			void unlisten.then((f) => f());
		};
	}, []);
	return {
		on: settings?.airGap ?? null,
		set: (on) => setSettings((s) => (s ? { ...s, airGap: on } : s)),
		error,
		shortcut: settings?.shortcutRegistered
			? formatShortcut(settings.shortcut)
			: null,
	};
}

export function SettingsWindow() {
	const [pane, setPane] = useState<SettingsPane>(() => {
		const saved = localStorage.getItem(LAST_PANE);
		return isPane(saved) ? saved : "general";
	});
	const [trayQr, setTrayQr] = useState<Qr | null>(null);
	const airGap = useAirGap();

	useEffect(() => localStorage.setItem(LAST_PANE, pane), [pane]);
	useEffect(() => {
		const paneEvent = listen<SettingsPane>(PANE_EVENT, (e) => {
			if (isPane(e.payload)) setPane(e.payload);
		});
		const qr = listen<SponsorLink>(QR_EVENT, (e) => {
			if (e.payload.kind === "qr") setTrayQr(e.payload);
		});
		return () => {
			void paneEvent.then((f) => f());
			void qr.then((f) => f());
		};
	}, []);

	const title = t(TABS.find(([p]) => p === pane)?.[1] ?? "settings.general");
	const panes: Record<SettingsPane, React.ReactNode> = {
		general: (
			<>
				<UpdatePanel airGap={airGap} />
				<DataPanel />
			</>
		),
		folders: <FolderAndSchedulePanel />,
		models: <ModelsPanel airGap={airGap} />,
		privacy: <AirGapPanel airGap={airGap} />,
		shortcuts: (
			<>
				<ShortcutSetting />
				<Group title={t("shortcuts.title")}>
					<div className="form-row">
						<ShortcutTable quickSearch={airGap.shortcut} />
					</div>
				</Group>
			</>
		),
		appearance: <ThemeSetting />,
		sponsor: <SponsorPanel trayQr={trayQr} />,
		about: <AboutPanel />,
	};

	return (
		<div className="settings">
			<header className="settings-chrome" data-tauri-drag-region>
				<div className="settings-title" data-tauri-drag-region>
					{title}
				</div>
				<div
					className="settings-tabs"
					role="tablist"
					aria-label={t("settings.tabs")}
					data-tauri-drag-region
				>
					{TABS.map(([id, label, Icon]) => (
						<button
							key={id}
							type="button"
							role="tab"
							id={`tab-${id}`}
							aria-selected={id === pane}
							aria-controls={`pane-${id}`}
							tabIndex={id === pane ? 0 : -1}
							className="settings-tab"
							onClick={() => setPane(id)}
							onKeyDown={(e) => {
								const i = TABS.findIndex(([p]) => p === pane);
								const step =
									e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
								if (!step) return;
								e.preventDefault();
								const next = TABS[(i + step + TABS.length) % TABS.length];
								if (!next) return;
								setPane(next[0]);
								document.getElementById(`tab-${next[0]}`)?.focus();
							}}
						>
							<Icon size={20} strokeWidth={1.5} aria-hidden />
							{t(label)}
						</button>
					))}
				</div>
			</header>
			{TABS.map(([id]) => (
				<div
					key={id}
					id={`pane-${id}`}
					role="tabpanel"
					aria-labelledby={`tab-${id}`}
					className="settings-pane"
					hidden={id !== pane}
				>
					{panes[id]}
				</div>
			))}
		</div>
	);
}
