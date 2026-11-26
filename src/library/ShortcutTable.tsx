// SPDX-License-Identifier: GPL-3.0-or-later
// The app's keyboard shortcuts, shown in Help › Keyboard Shortcuts and Settings › Shortcuts.
// Keep in step with the menu accelerators in src-tauri/src/desktop/menu.rs.
import { type MessageKey, t } from "../i18n";
import { KeyboardShortcut, type ShortcutKeys } from "../ui/KeyboardShortcut";

const ROWS: readonly [MessageKey, ShortcutKeys][] = [
	["shortcuts.find", { meta: true, key: "f" }],
	["shortcuts.sections", { meta: true, key: "1–4" }],
	["shortcuts.back", { meta: true, key: "[" }],
	["shortcuts.forward", { meta: true, key: "]" }],
	["shortcuts.open", { key: "Enter" }],
	["shortcuts.reveal", { meta: true, key: "Enter" }],
	["shortcuts.copy", { meta: true, key: "c" }],
	["shortcuts.addFolder", { meta: true, key: "o" }],
	["shortcuts.sidebar", { ctrl: true, meta: true, key: "s" }],
	["shortcuts.fullScreen", { ctrl: true, meta: true, key: "f" }],
	["shortcuts.settings", { meta: true, key: "," }],
	["shortcuts.help", { meta: true, key: "/" }],
	["shortcuts.close", { meta: true, key: "w" }],
];

/** `quickSearch` is the global shortcut as the user set it, already formatted. */
export function ShortcutTable({ quickSearch }: { quickSearch: string | null }) {
	return (
		<dl className="shortcut-table">
			{quickSearch && (
				<div className="shortcut-table-row">
					<dt>
						{t("shortcuts.quickSearch")}{" "}
						<span className="shortcut-table-note">{t("shortcuts.global")}</span>
					</dt>
					<dd>
						<kbd className="kbd">{quickSearch}</kbd>
					</dd>
				</div>
			)}
			{ROWS.map(([label, keys]) => (
				<div key={label} className="shortcut-table-row">
					<dt>{t(label)}</dt>
					<dd>
						<KeyboardShortcut keys={keys} />
					</dd>
				</div>
			))}
		</dl>
	);
}
