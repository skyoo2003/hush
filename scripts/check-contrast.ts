// SPDX-License-Identifier: GPL-3.0-or-later
// Checks that every fg/bg semantic pair used for text (4.5:1) and the focus ring (3:1) meets
// WCAG AA in every theme of src/ui/tokens.css, with and without Increase Contrast. Also fails if
// the system-dark block drifts from [data-theme="dark"].
// Run: node scripts/check-contrast.ts
import { readFileSync } from "node:fs";
import {
	contrast,
	mix,
	over,
	parseHex,
	type Rgba,
} from "../src/ui/contrast.ts";

type Decls = Map<string, string>;

/** Declarations of tokens.css by selector, `@media …` prefixed; whitespace collapsed. */
function parse(css: string): Map<string, Decls> {
	const blocks = new Map<string, Decls>();
	const stack: string[] = [];
	const text = css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
	for (const [, raw, end] of text.matchAll(/([^{};]*)([{};])/g)) {
		const part = (raw ?? "").trim();
		if (end === "{") stack.push(part);
		else if (end === "}") stack.pop();
		else if (part) {
			const i = part.indexOf(":");
			const key = stack.join(" ");
			if (!blocks.has(key)) blocks.set(key, new Map());
			blocks.get(key)?.set(part.slice(0, i).trim(), part.slice(i + 1).trim());
		}
	}
	return blocks;
}

const TRANSPARENT: Rgba = { r: 0, g: 0, b: 0, a: 0 };

function resolve(decls: Decls, value: string, seen: string[] = []): Rgba {
	const v = value.trim();
	const ref = /^var\((--[\w-]+)\)$/.exec(v)?.[1];
	if (ref) {
		const next = decls.get(ref);
		if (next === undefined || seen.includes(ref))
			throw new Error(`cannot resolve ${[...seen, ref].join(" -> ")}`);
		return resolve(decls, next, [...seen, ref]);
	}
	if (v === "transparent") return TRANSPARENT;
	const m = /^color-mix\(\s*in srgb, (.+) (\d+(?:\.\d+)?)%, (.+?)\s*\)$/.exec(v);
	if (m?.[1] && m[2] && m[3])
		return mix(
			resolve(decls, m[1], seen),
			Number(m[2]) / 100,
			resolve(decls, m[3], seen),
		);
	const hex = parseHex(v);
	if (hex) return hex;
	throw new Error(`unsupported color value: ${v}`);
}

// [foreground, background, minimum ratio]. A background with alpha is painted over --bg-content.
// Text needs 4.5:1; the focus ring is a non-text indicator and needs 3:1 (WCAG 1.4.11).
const TEXT = ["--fg-primary", "--fg-secondary", "--fg-tertiary"];
const SURFACES = ["--bg-window", "--bg-content", "--bg-elevated", "--bg-hover"];
const STATUS = ["--success", "--warning", "--danger"];
const PAIRS: [string, string, number][] = [
	...TEXT.flatMap((fg) =>
		SURFACES.map((bg): [string, string, number] => [fg, bg, 4.5]),
	),
	...STATUS.flatMap((fg) =>
		["--bg-content", "--bg-elevated"].map((bg): [string, string, number] => [
			fg,
			bg,
			4.5,
		]),
	),
	["--fg-on-accent", "--accent", 4.5],
	["--fg-on-accent", "--bg-selected", 4.5],
	["--fg-primary", "--bg-selected-inactive", 4.5],
	["--fg-secondary", "--bg-selected-inactive", 4.5],
	["--fg-primary", "--accent-soft", 4.5],
	...[...SURFACES, "--accent-soft"].map((bg): [string, string, number] => [
		"--accent-text",
		bg,
		4.5,
	]),
	["--fg-primary", "--highlight-mark", 4.5],
	...SURFACES.map((bg): [string, string, number] => ["--focus-ring", bg, 3]),
];

const file = new URL("../src/ui/tokens.css", import.meta.url);
const blocks = parse(readFileSync(file, "utf8"));
const block = (selector: string): Decls => {
	const b = blocks.get(selector);
	if (!b) throw new Error(`tokens.css has no block ${selector}`);
	return b;
};
const base = block(":root");
const systemDark = block(
	'@media (prefers-color-scheme: dark) :root:is([data-theme="system"], :not([data-theme]))',
);
const more = block("@media (prefers-contrast: more) :root:is(*, [data-theme])");

const THEMES: Record<string, Decls[]> = {
	"system (light)": [],
	"system (dark)": [systemDark],
	light: [block(':root[data-theme="light"]')],
	dark: [block(':root[data-theme="dark"]')],
	oled_dark: [block(':root[data-theme="oled_dark"]')],
	terminal: [block(':root[data-theme="terminal"]')],
};

const failures: string[] = [];
const dark = block(':root[data-theme="dark"]');
if (JSON.stringify([...dark]) !== JSON.stringify([...systemDark]))
	failures.push(
		'the prefers-color-scheme: dark block differs from [data-theme="dark"]',
	);

let checked = 0;
for (const [theme, layers] of Object.entries(THEMES)) {
	for (const increase of [false, true]) {
		const decls: Decls = new Map(
			[base, ...layers, ...(increase ? [more] : [])].flatMap((d) => [...d]),
		);
		const content = resolve(decls, "var(--bg-content)");
		for (const [fg, bg, min] of PAIRS) {
			const b = over(resolve(decls, `var(${bg})`), content);
			const f = over(resolve(decls, `var(${fg})`), b);
			const ratio = contrast(f, b);
			checked++;
			if (ratio < min)
				failures.push(
					`${theme}${increase ? " + Increase Contrast" : ""}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1, needs ${min}:1`,
				);
		}
	}
}

if (failures.length) {
	console.error(failures.join("\n"));
	process.exit(1);
}
console.log(
	`check-contrast: ${checked} pairs pass in ${Object.keys(THEMES).length} themes × 2 contrast settings`,
);
