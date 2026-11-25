// SPDX-License-Identifier: GPL-3.0-or-later
// WCAG 2 contrast on sRGB colors, shared by appearance.ts (text on the system accent) and
// scripts/check-contrast.ts (every theme in tokens.css).

/** sRGB channels 0..1 and alpha 0..1. */
export type Rgba = { r: number; g: number; b: number; a: number };

export function parseHex(hex: string): Rgba | null {
	const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
	if (!m?.[1]) return null;
	const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
	const n = (i: number) => Number.parseInt(h.slice(i, i + 2), 16) / 255;
	return { r: n(0), g: n(2), b: n(4), a: 1 };
}

/** CSS `color-mix(in srgb, x p, y)`, with p in 0..1 (premultiplied alpha, as CSS does). */
export function mix(x: Rgba, p: number, y: Rgba): Rgba {
	const a = x.a * p + y.a * (1 - p);
	if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
	const ch = (k: "r" | "g" | "b") =>
		(x[k] * x.a * p + y[k] * y.a * (1 - p)) / a;
	return { r: ch("r"), g: ch("g"), b: ch("b"), a };
}

/** `top` painted over an opaque `under`. */
export function over(top: Rgba, under: Rgba): Rgba {
	const ch = (k: "r" | "g" | "b") => top[k] * top.a + under[k] * (1 - top.a);
	return { r: ch("r"), g: ch("g"), b: ch("b"), a: 1 };
}

function luminance({ r, g, b }: Rgba): number {
	const lin = (c: number) =>
		c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Contrast ratio of two opaque colors, 1..21. */
export function contrast(x: Rgba, y: Rgba): number {
	const [hi, lo] = [luminance(x), luminance(y)].sort((p, q) => q - p);
	return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}
