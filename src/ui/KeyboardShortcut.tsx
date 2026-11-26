// SPDX-License-Identifier: GPL-3.0-or-later
import "./KeyboardShortcut.css";

export type ShortcutKeys = {
	ctrl?: boolean;
	alt?: boolean;
	shift?: boolean;
	meta?: boolean;
	/** A `KeyboardEvent.key` value. */
	key: string;
};

export type KeyboardShortcutProps = {
	keys: ShortcutKeys;
	/** Spoken name, e.g. "Command Shift K"; the glyphs alone read poorly. */
	label?: string;
};

const KEY_GLYPHS: Record<string, string> = {
	Enter: "↩",
	Escape: "⎋",
	Backspace: "⌫",
	Tab: "⇥",
	ArrowUp: "↑",
	ArrowDown: "↓",
	ArrowLeft: "←",
	ArrowRight: "→",
	" ": "␣",
};

/** Glyphs in macOS order: ⌃⌥⇧⌘, then the key. */
export function shortcutGlyphs({ ctrl, alt, shift, meta, key }: ShortcutKeys) {
	const keyGlyph =
		KEY_GLYPHS[key] ?? (key.length === 1 ? key.toUpperCase() : key);
	return `${ctrl ? "⌃" : ""}${alt ? "⌥" : ""}${shift ? "⇧" : ""}${meta ? "⌘" : ""}${keyGlyph}`;
}

export function KeyboardShortcut({ keys, label }: KeyboardShortcutProps) {
	return (
		<kbd className="kbd" role={label ? "img" : undefined} aria-label={label}>
			{shortcutGlyphs(keys)}
		</kbd>
	);
}
