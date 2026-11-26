// SPDX-License-Identifier: GPL-3.0-or-later
// Draws the tray template images (src-tauri/icons/tray/*.png) from Lucide shapes (ISC) in WebKit:
// 36x36 px (18 pt at 2x), black strokes on transparent, so macOS can tint them.
// Run: node scripts/make-tray-icons.mjs
import { mkdirSync } from "node:fs";
import { webkit } from "playwright";

const icon = async (name) =>
  (await import(`lucide-react/dist/esm/icons/${name}.mjs`)).__iconData.node;
const shield = (await icon("shield"))[0];
const glyphs = {
  idle: await icon("shield-check"),
  indexing: await icon("shield-ellipsis"),
  paused: [shield, ["path", { d: "M10 9v6" }], ["path", { d: "M14 9v6" }]],
  "airgap-off": await icon("shield-off"),
};

const svg = (nodes) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${nodes
    .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).filter(([k]) => k !== "key").map(([k, v]) => `${k}="${v}"`).join(" ")}/>`)
    .join("")}</svg>`;

const out = new URL("../src-tauri/icons/tray/", import.meta.url).pathname;
mkdirSync(out, { recursive: true });
const browser = await webkit.launch();
const page = await browser.newPage({ viewport: { width: 36, height: 36 }, deviceScaleFactor: 1 });
for (const [name, nodes] of Object.entries(glyphs)) {
  await page.setContent(`<body style="margin:0;background:transparent">${svg(nodes)}</body>`);
  await page.locator("svg").screenshot({ path: `${out}${name}.png`, omitBackground: true });
}
await browser.close();
