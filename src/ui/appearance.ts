// SPDX-License-Identifier: GPL-3.0-or-later
// Applies the theme and accent from Rust (`appearance.rs`) to <html>: `data-theme`, and for the
// system accent, --accent and a readable --fg-on-accent. tokens.css derives everything else.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Appearance } from "../ipc/Appearance";
import { contrast, parseHex } from "./contrast";

// Must match `appearance::ACCENT_EVENT` and `appearance::APPEARANCE_EVENT`.
export const ACCENT_EVENT = "ui://accent";
export const APPEARANCE_EVENT = "ui://appearance";

const WHITE = { r: 1, g: 1, b: 1, a: 1 };

/** White text on the accent, as macOS draws selections, unless white falls under 3:1 (the
 * yellow, orange, and green accents); then black. macOS Blue with white is 4.0:1. */
export function onAccent(accent: string): "white" | "black" | null {
	const c = parseHex(accent);
	if (!c) return null;
	return contrast(WHITE, c) >= 3 ? "white" : "black";
}

export function applyAppearance(
	a: Appearance,
	root = document.documentElement,
) {
	root.dataset.theme = a.theme;
	const accent = a.accent === "system" ? a.systemAccent : null;
	const text = accent ? onAccent(accent) : null;
	if (accent && text) {
		root.style.setProperty("--accent", accent);
		root.style.setProperty("--fg-on-accent", `var(--prim-${text})`);
	} else {
		root.style.removeProperty("--accent");
		root.style.removeProperty("--fg-on-accent");
	}
}

/** Applies the saved appearance, then follows setting and system accent changes. */
export async function startAppearance() {
	let current = await invoke<Appearance>("get_appearance");
	const apply = (a: Appearance) => {
		current = a;
		applyAppearance(a);
	};
	apply(current);
	await listen<Appearance>(APPEARANCE_EVENT, (e) => apply(e.payload));
	await listen<string>(ACCENT_EVENT, (e) =>
		apply({ ...current, systemAccent: e.payload }),
	);
}
