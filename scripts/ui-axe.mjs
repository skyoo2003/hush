// SPDX-License-Identifier: GPL-3.0-or-later
// Runs axe-core (every rule, color-contrast included) on each Components harness state in WebKit,
// light and dark, en and ko. Prints each violation and exits non-zero if there is any.
// An argument keeps only names that start with it, e.g. `pnpm ui:axe components-overlays/menu`.
import { createRequire } from "node:module";
import { webkit } from "playwright";
import { createServer } from "vite";

const only = process.argv[2] ?? "components";
const axePath = createRequire(import.meta.url).resolve("axe-core");

process.env.VITE_MOCK_IPC = "1";
const server = await createServer({
  root: new URL("..", import.meta.url).pathname,
  logLevel: "warn",
  server: { port: 1422, strictPort: false },
});
await server.listen();
const base = server.resolvedUrls.local[0].replace(/\/$/, "");

const browser = await webkit.launch();
const problems = [];
let violations = 0;
try {
  const context = await browser.newContext({
    viewport: { width: 1200, height: 860 },
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(`${base}/__harness`);
  const states = await page
    .locator("a[data-scenario]")
    .evaluateAll((as) => as.map((a) => ({ name: a.dataset.scenario, href: a.getAttribute("href") })));
  const todo = states.filter((s) => s.name.startsWith(only));
  if (todo.length === 0) throw new Error(`no harness state starts with "${only}"`);

  for (const { name, href } of todo) {
    await page.emulateMedia({ colorScheme: name.includes("-dark-") ? "dark" : "light" });
    await page.goto(base + href);
    const html = page.frameLocator("#window").locator("html[data-harness]");
    await html.waitFor({ state: "attached", timeout: 15_000 }).catch(() => {});
    const status = (await html.getAttribute("data-harness").catch(() => null)) ?? "timed out";
    if (status !== "ready") {
      problems.push(`${name}: ${status}`);
      continue;
    }
    const frame = page.frame({ url: /\/__harness\/app/ });
    if (!frame) throw new Error(`${name}: no app frame`);
    await frame.addScriptTag({ path: axePath });
    const found = await frame.evaluate(async () => {
      // Check the app's own accent. The system accent is the user's macOS choice, which the app
      // follows with a 3:1 floor (appearance.ts: macOS Blue with white is 4.0:1).
      document.documentElement.style.removeProperty("--accent");
      document.documentElement.style.removeProperty("--fg-on-accent");
      // Colors fade even with reduced motion (base.css); let the accent change settle.
      await new Promise((r) => setTimeout(r, 400));
      // A modal layer (sheet, alert, menu, select) hides the page from assistive tech and traps
      // focus, so only what is still exposed is checked; page-level rules then do not apply.
      const exposed = [...document.body.children].filter(
        (el) => el.tagName !== "SCRIPT" && el.getAttribute("aria-hidden") !== "true",
      );
      const modal = document.querySelector("#page[aria-hidden=true]");
      const result = await window.axe.run(modal ? { include: exposed } : document, {
        // Radix portals popups to <body>; each belongs to the trigger that opened it.
        checks: { region: { options: { regionMatcher: "[data-radix-popper-content-wrapper]" } } },
      });
      return result.violations.map((v) => ({
        id: v.id,
        help: v.help,
        nodes: v.nodes.map((n) => `${n.target.join(" ")}${n.failureSummary ? ` (${n.failureSummary.split("\n").slice(1).join("; ").trim()})` : ""}`),
      }));
    });
    const count = found.reduce((n, v) => n + v.nodes.length, 0);
    violations += count;
    console.log(`${name}: ${count} violation${count === 1 ? "" : "s"}`);
    for (const v of found) {
      console.log(`  ${v.id}: ${v.help}`);
      for (const n of v.nodes) console.log(`    ${n}`);
    }
  }
  console.log(`ui-axe: ${todo.length - problems.length}/${todo.length} states checked, ${violations} violations`);
} finally {
  await browser.close();
  await server.close();
}
if (problems.length) console.error(problems.join("\n"));
if (problems.length || violations) process.exit(1);
