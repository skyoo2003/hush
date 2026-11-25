// SPDX-License-Identifier: GPL-3.0-or-later
// Photographs every UI harness state (src/dev/scenarios.ts) in WebKit, the engine behind WKWebView:
// ui-snapshots/{screen}/{state}-{theme}-{lang}.png. An argument keeps only names that start with it,
// e.g. `pnpm ui:snap quick-search/results`. Time, randomness, caret, and animations are frozen.
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { webkit } from "playwright";
import { createServer } from "vite";

const only = process.argv[2] ?? "";
const out = new URL("../ui-snapshots/", import.meta.url).pathname;

process.env.VITE_MOCK_IPC = "1";
const server = await createServer({
  root: new URL("..", import.meta.url).pathname,
  logLevel: "warn",
  server: { port: 1421, strictPort: false },
});
await server.listen();
const base = server.resolvedUrls.local[0].replace(/\/$/, "");

const browser = await webkit.launch();
const failures = [];
try {
  const context = await browser.newContext({
    viewport: { width: 1200, height: 860 },
    deviceScaleFactor: 2,
    timezoneId: "Asia/Seoul",
    reducedMotion: "reduce",
  });
  // mulberry32 with a fixed seed, in every frame.
  await context.addInitScript(() => {
    let a = 42;
    Math.random = () => {
      a = (a + 0x6d2b79f5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  });
  const page = await context.newPage();
  await page.clock.setFixedTime(new Date("2026-11-26T10:00:00+09:00"));
  await page.goto(`${base}/__harness`);
  const shots = await page
    .locator("a[data-scenario]")
    .evaluateAll((as) => as.map((a) => ({ name: a.dataset.scenario, href: a.getAttribute("href") })));
  const todo = shots.filter((s) => s.name.startsWith(only));
  if (todo.length === 0) throw new Error(`no harness state starts with "${only}"`);

  for (const { name, href } of todo) {
    const theme = name.includes("-dark-") ? "dark" : "light";
    const contrast = name.includes("increase-contrast") ? "more" : "no-preference";
    await page.emulateMedia({ colorScheme: theme, contrast });
    await page.goto(base + href);
    const frame = page.frameLocator("#window");
    const html = frame.locator("html[data-harness]");
    await html.waitFor({ state: "attached", timeout: 15_000 }).catch(() => {});
    const status = (await html.getAttribute("data-harness").catch(() => null)) ?? "timed out";
    if (status !== "ready") {
      failures.push(`${name}: ${status}`);
      continue;
    }
    const path = `${out}${name}.png`;
    mkdirSync(dirname(path), { recursive: true });
    await page.locator("#window").screenshot({ path, animations: "disabled", caret: "hide" });
  }
  console.log(`ui-snap: ${todo.length - failures.length}/${todo.length} screenshots in ui-snapshots/`);
} finally {
  await browser.close();
  await server.close();
}
if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
