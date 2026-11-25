// SPDX-License-Identifier: GPL-3.0-or-later
// Harness page (/__harness, screen "tokens"): every token of src/ui/tokens.css in the current
// theme. The pickers go through the same path as the app: set_theme / set_accent answer with
// `ui://appearance`, a system accent change arrives as `ui://accent`, and appearance.ts applies both.
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { type ReactNode, useEffect, useReducer } from "react";
import type { Accent } from "../ipc/Accent";
import type { Theme } from "../ipc/Theme";
import { TEXT_VARIANTS, Text } from "../ui/Text";
import "./tokens-page.css";

const THEMES: Theme[] = ["system", "light", "dark", "oled_dark", "terminal"];
const ACCENTS: Accent[] = ["system", "brand"];
/** The macOS accent choices, as NSColor.controlAccentColor reports them in light mode. */
const SYSTEM_ACCENTS: Record<string, string> = {
	blue: "#007aff",
	purple: "#953d96",
	pink: "#e0457b",
	red: "#e0393e",
	orange: "#f7821b",
	yellow: "#ffc600",
	green: "#62ba46",
	graphite: "#8c8c8c",
};

const SURFACES = [
	"--bg-window",
	"--bg-content",
	"--bg-elevated",
	"--bg-hover",
	"--bg-selected",
	"--bg-selected-inactive",
	"--accent-soft",
	"--highlight-mark",
];
const FILLS = [
	"--bg-sidebar",
	"--border-subtle",
	"--border-strong",
	"--accent",
	"--focus-ring",
	"--overlay-bbox",
];
const FOREGROUNDS = [
	"--fg-primary",
	"--fg-secondary",
	"--fg-tertiary",
	"--fg-on-accent",
	"--success",
	"--warning",
	"--danger",
];
const SPACES = [4, 8, 12, 16, 20, 24, 32];
const RADII = ["control", "card", "panel"];
const SHADOWS = ["popover", "panel"];
const MOTION = [
	"--duration-fast",
	"--duration-normal",
	"--ease-out",
	"--focus-ring-width",
	"--font-text",
	"--font-mono",
];

/** Re-renders when <html> changes theme or accent, so resolved values stay current. */
function useRootChanges() {
	const [, bump] = useReducer((n: number) => n + 1, 0);
	useEffect(() => {
		const o = new MutationObserver(bump);
		o.observe(document.documentElement, { attributes: true });
		return () => o.disconnect();
	}, []);
}

const rootValue = (name: string) =>
	getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** A token's color as the browser resolves it (var() and color-mix() included). */
function Resolved({ token }: { token: string }) {
	return (
		<span
			className="tp-resolved"
			ref={(el) => {
				if (!el) return;
				// outline-color resolves the color without painting anything (no outline-style).
				el.style.setProperty("outline-color", `var(${token})`);
				el.dataset.value = getComputedStyle(el).outlineColor;
			}}
		/>
	);
}

function Section({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="tp-section">
			<Text as="h2" variant="headline">
				{title}
			</Text>
			{children}
		</section>
	);
}

function Picker<T extends string>(props: {
	id: string;
	label: string;
	options: T[];
	onPick: (v: T) => void;
}) {
	return (
		<label>
			<Text variant="callout" tone="secondary">
				{props.label}
			</Text>{" "}
			<select
				id={props.id}
				onChange={(e) => props.onPick(e.target.value as T)}
				defaultValue=""
			>
				<option value="" disabled>
					…
				</option>
				{props.options.map((o) => (
					<option key={o} value={o}>
						{o}
					</option>
				))}
			</select>
		</label>
	);
}

export function TokensPage() {
	useRootChanges();
	const root = document.documentElement;
	return (
		<main className="tp">
			<header className="tp-header">
				<Text as="h1" variant="title2">
					Tokens · {root.dataset.theme ?? "no data-theme"} · accent{" "}
					{root.style.getPropertyValue("--accent") || "brand"}
				</Text>
				<div className="tp-row">
					<Picker
						id="pick-theme"
						label="Theme"
						options={THEMES}
						onPick={(theme) => invoke("set_theme", { theme })}
					/>
					<Picker
						id="pick-accent"
						label="Accent"
						options={ACCENTS}
						onPick={(accent) => invoke("set_accent", { accent })}
					/>
					<Picker
						id="pick-system-accent"
						label="System accent (simulated)"
						options={Object.keys(SYSTEM_ACCENTS)}
						onPick={(k) => emit("ui://accent", SYSTEM_ACCENTS[k])}
					/>
				</div>
			</header>

			<Section title="Surfaces, with each text tone">
				<div className="tp-grid">
					{SURFACES.map((bg) => (
						<div
							key={bg}
							className={bg === "--bg-selected" ? "tp-surface tp-on-accent" : "tp-surface"}
							style={{ background: `var(${bg})` }}
						>
							<code>{bg}</code>
							<Resolved token={bg} />
							{bg === "--bg-selected" ? (
								<span style={{ color: "var(--fg-on-accent)" }}>
									On accent text
								</span>
							) : (
								<>
									<Text tone="primary">Primary 본문</Text>
									<Text tone="secondary">Secondary 보조</Text>
									<Text tone="tertiary">Tertiary 3차</Text>
								</>
							)}
						</div>
					))}
				</div>
			</Section>

			<Section title="Fills and borders">
				<div className="tp-grid">
					{FILLS.map((c) => (
						<div key={c} className="tp-chip">
							<span className="tp-swatch" style={{ background: `var(${c})` }} />
							<code>{c}</code>
							<Resolved token={c} />
						</div>
					))}
				</div>
			</Section>

			<Section title="Foregrounds on --bg-content">
				<div className="tp-grid">
					{FOREGROUNDS.map((c) => (
						<div key={c} className="tp-chip">
							<span
								style={{
									color: `var(${c})`,
									fontWeight: "var(--text-headline-weight)",
								}}
							>
								Aa 가나
							</span>
							<code>{c}</code>
							<Resolved token={c} />
						</div>
					))}
				</div>
			</Section>

			<Section title="Type scale">
				{TEXT_VARIANTS.map((v) => (
					<div key={v} className="tp-type">
						<code>{v}</code>
						<Text variant={v}>
							Cited answers from local files · 로컬 문서의 출처
						</Text>
						<Text variant="footnote" tone="tertiary">
							{rootValue(`--text-${v}-size`)} / {rootValue(`--text-${v}-line`)}{" "}
							/ {rootValue(`--text-${v}-weight`)}
						</Text>
					</div>
				))}
				<div className="tp-type">
					<code>mono</code>
					<code>~/Documents/Receipts/2026-03.pdf · p.4</code>
				</div>
			</Section>

			<div className="tp-columns">
				<Section title="Spacing">
					{SPACES.map((n) => (
						<div key={n} className="tp-row">
							<span
								className="tp-space"
								style={{ width: `var(--space-${n})` }}
							/>
							<code>--space-{n}</code>
						</div>
					))}
				</Section>
				<Section title="Radii">
					<div className="tp-row">
						{RADII.map((r) => (
							<div
								key={r}
								className="tp-radius"
								style={{ borderRadius: `var(--radius-${r})` }}
							>
								<code>--radius-{r}</code>
							</div>
						))}
					</div>
				</Section>
				<Section title="Shadows">
					<div className="tp-row">
						{SHADOWS.map((s) => (
							<div
								key={s}
								className="tp-shadow"
								style={{ boxShadow: `var(--shadow-${s})` }}
							>
								<code>--shadow-{s}</code>
							</div>
						))}
					</div>
				</Section>
			</div>

			<Section title="Motion, focus, fonts">
				{MOTION.map((m) => (
					<div key={m} className="tp-row">
						<code>{m}</code>
						<Text tone="secondary">{rootValue(m)}</Text>
					</div>
				))}
				<div className="tp-row">
					<button type="button" className="tp-focused">
						Focus ring
					</button>
					<mark>search hit</mark>
					<span className="tp-bbox">viewer bbox</span>
				</div>
			</Section>
		</main>
	);
}
