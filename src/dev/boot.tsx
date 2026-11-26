// SPDX-License-Identifier: GPL-3.0-or-later
// UI harness entry. The `mock-ipc` Vite plugin puts this script before main.tsx in dev with
// VITE_MOCK_IPC=1, so it runs first:
//   /__harness                          the list of every screen × state
//   /__harness?screen=&state=&theme=&lang=   one state in a frame the size of the real window
//   /__harness/app?screen=&state=&lang=      the real App on mocked IPC (the frame's content)
// It sets `data-harness` on <html> to "ready" once the state is reached, or "error: …".
import { createRoot } from "react-dom/client";
import { DEBOUNCE_MS } from "../search";
import { baseFixture } from "./fixtures/state";
import { installMockIpc } from "./mockIpc";
import { TokensPage } from "./TokensPage";
import {
	LANGS,
	type Lang,
	SCENARIOS,
	type Scenario,
	type Step,
	scenario,
	THEMES,
	WINDOWS,
} from "./scenarios";

const params = new URL(location.href).searchParams;
const lang: Lang = params.get("lang") === "ko" ? "ko" : "en";
const theme = params.get("theme") === "dark" ? "dark" : "light";
const root = document.documentElement;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitFor(selector: string): Promise<HTMLElement> {
	for (let i = 0; i < 150; i++) {
		const el = document.querySelector<HTMLElement>(selector);
		if (el) return el;
		await sleep(20);
	}
	throw new Error(`no element matches ${selector}`);
}

async function run(step: Step) {
	if ("fill" in step) {
		const el = (await waitFor(step.fill)) as HTMLInputElement;
		el.focus();
		const text = typeof step.text === "string" ? step.text : step.text[lang];
		// React tracks the value through the prototype setter; assigning `el.value` skips onChange.
		Object.getOwnPropertyDescriptor(
			HTMLInputElement.prototype,
			"value",
		)?.set?.call(el, text);
		el.dispatchEvent(new Event("input", { bubbles: true }));
		await sleep(DEBOUNCE_MS);
	} else if ("select" in step) {
		const el = (await waitFor(step.select)) as HTMLSelectElement;
		el.value = step.value;
		el.dispatchEvent(new Event("change", { bubbles: true }));
	} else if ("click" in step) {
		(await waitFor(step.click)).click();
	} else if ("key" in step) {
		const opts = {
			key: step.key,
			metaKey: step.meta ?? false,
			bubbles: true,
			cancelable: true,
		};
		document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", opts));
	} else if ("focus" in step) {
		(await waitFor(step.focus)).focus();
	} else if ("context" in step) {
		const el = await waitFor(step.context);
		const r = el.getBoundingClientRect();
		el.dispatchEvent(
			new MouseEvent("contextmenu", {
				bubbles: true,
				cancelable: true,
				button: 2,
				clientX: r.left + r.width / 2,
				clientY: r.top + r.height / 2,
			}),
		);
	} else if ("scroll" in step) {
		(await waitFor(step.scroll)).scrollIntoView({ block: "start" });
	} else {
		await waitFor(step.wait);
	}
	await sleep(0);
}

function startApp(s: Scenario) {
	// i18n reads navigator.language when main.tsx imports it, which happens after this script.
	Object.defineProperty(navigator, "language", {
		get: () => (lang === "ko" ? "ko-KR" : "en-US"),
	});
	root.lang = lang;
	const f = baseFixture();
	s.setup?.(f);
	const ipc = installMockIpc(WINDOWS[s.screen].label, f);
	const settle = async () => {
		// Settled: no command in flight for 300ms (longer than the search debounce).
		for (let quiet = 0; quiet < 300; quiet = ipc.idle() ? quiet + 20 : 0)
			await sleep(20);
	};
	// Harness pages replace App; main.tsx still loads the styles and applies the appearance.
	const page = s.screen === "tokens" || s.screen.startsWith("components");
	const mount = page ? "#page > *" : "#root > *";
	if (page) {
		document.getElementById("root")?.remove();
		const el = document.body.appendChild(document.createElement("div"));
		el.id = "page";
		if (s.screen === "tokens") createRoot(el).render(<TokensPage />);
		// Imported late so i18n reads the navigator.language set above.
		else
			import("./ComponentsPage").then(({ ComponentsPage }) =>
				createRoot(el).render(
					<ComponentsPage overlays={s.screen === "components-overlays"} />,
				),
			);
	}
	window.addEventListener("load", async () => {
		try {
			// A background frame would gray every column; the page forces inactive where it wants it.
			if (s.screen.startsWith("components")) root.dataset.window = "active";
			await waitFor(mount);
			await settle();
			await ipc.emitEvents();
			for (const step of s.steps ?? []) await run(step);
			await settle();
			root.dataset.harness = "ready";
		} catch (e) {
			root.dataset.harness = `error: ${e instanceof Error ? e.message : String(e)}`;
		}
	});
}

const href = (s: Scenario, t: string, l: string) =>
	`/__harness?screen=${s.screen}&state=${s.state}&theme=${t}&lang=${l}`;

function List() {
	return (
		<main style={{ font: "13px -apple-system, system-ui", padding: 16 }}>
			<h1>UI harness</h1>
			<p>
				Every screen × state on mocked IPC. <code>pnpm ui:snap</code>{" "}
				photographs each one.
			</p>
			{Object.keys(WINDOWS).map((screen) => (
				<table
					key={screen}
					style={{ borderCollapse: "collapse", marginBottom: 24 }}
				>
					<caption style={{ textAlign: "left", fontWeight: 600 }}>
						{screen}
					</caption>
					<tbody>
						{SCENARIOS.filter((s) => s.screen === screen).map((s) => (
							<tr key={s.state}>
								<td style={{ paddingRight: 16 }}>{s.state}</td>
								<td style={{ paddingRight: 16, color: "GrayText" }}>
									{s.note}
								</td>
								{THEMES.flatMap((t) =>
									WINDOWS[s.screen].langs.map((l) => (
										<td key={t + l} style={{ paddingRight: 8 }}>
											<a
												data-scenario={`${s.screen}/${s.state}-${t}-${l}`}
												href={href(s, t, l)}
											>
												{t}·{l}
											</a>
										</td>
									)),
								)}
							</tr>
						))}
					</tbody>
				</table>
			))}
		</main>
	);
}

function Frame({ s }: { s: Scenario }) {
	const w = WINDOWS[s.screen];
	return (
		<div style={{ font: "13px -apple-system, system-ui", padding: 16 }}>
			<p>
				<a href="/__harness">All states</a> · {s.screen} / {s.state} ({w.width}×
				{w.height}) ·{" "}
				{THEMES.map((t) => (
					<a key={t} href={href(s, t, lang)} style={{ marginRight: 8 }}>
						{t}
					</a>
				))}
				{LANGS.map((l) => (
					<a key={l} href={href(s, theme, l)} style={{ marginRight: 8 }}>
						{l}
					</a>
				))}
			</p>
			<iframe
				id="window"
				title={`${s.screen} ${s.state}`}
				src={`/__harness/app?screen=${s.screen}&state=${s.state}&lang=${lang}`}
				width={w.width}
				height={w.height}
				// Sets prefers-color-scheme inside the frame (WebKit and Chromium honor it).
				style={{
					colorScheme: theme,
					border: 0,
					outline: "1px solid GrayText",
					display: "block",
				}}
			/>
		</div>
	);
}

const path = location.pathname.replace(/\/$/, "");
const s = scenario(params.get("screen"), params.get("state"));
if (path === "/__harness/app" && s) {
	startApp(s);
} else if (path !== "/__harness") {
	location.replace("/__harness");
} else {
	// main.tsx renders App into #root and App reads the window label when imported; give it a
	// label and take #root away so only the harness page renders here.
	installMockIpc("main", baseFixture());
	document.getElementById("root")?.remove();
	const el = document.body.appendChild(document.createElement("div"));
	createRoot(el).render(s ? <Frame s={s} /> : <List />);
}
