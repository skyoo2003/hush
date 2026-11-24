// SPDX-License-Identifier: GPL-3.0-or-later
// Fails if the frontend build (dist/) contains an http(s) URL outside the allow list. Run after `pnpm build`.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

// Prefixes the webview never loads; each names why it is in the bundle.
const ALLOW = [
  ["http://www.w3.org/", "XML namespace names in React DOM; identifiers, never fetched"],
  ["https://react.dev/errors/", "text of React's production error messages; never fetched"],
];

// Sponsor and site URLs: defined only in src/brand.ts (check-brand.sh), opened only in the external browser.
const brand = readFileSync(new URL("../src/brand.ts", import.meta.url), "utf8");
const brandUrls = brand
  .split("\n")
  .filter((l) => /^export const (SITE_URL|SPONSOR_URLS)\b/.test(l))
  .flatMap((l) => [...l.matchAll(/"(https:\/\/[^"]+)"/g)].map((m) => m[1]));
if (brandUrls.length === 0) throw new Error("check-dist-urls: no SITE_URL or SPONSOR_URLS in src/brand.ts");

const allowed = (url) => brandUrls.includes(url) || ALLOW.some(([prefix]) => url.startsWith(prefix));

const dist = new URL("../dist", import.meta.url).pathname;
const files = readdirSync(dist, { recursive: true, withFileTypes: true })
  .filter((e) => e.isFile())
  .map((e) => join(e.parentPath, e.name));
if (!files.some((f) => f.endsWith(".js"))) throw new Error("check-dist-urls: no JS in dist/; run pnpm build");

const bad = files.flatMap((f) =>
  [...readFileSync(f, "latin1").matchAll(/https?:\/\/[^\s"'`<>()\\]*/gi)]
    .map((m) => m[0])
    .filter((url) => !allowed(url))
    .map((url) => `${f.slice(dist.length + 1)}: ${url}`),
);
if (bad.length > 0) {
  console.error([...new Set(bad)].join("\n"));
  console.error("check-dist-urls: external URLs in dist/; remove them or add a reasoned entry to ALLOW");
  process.exit(1);
}
console.log(`check-dist-urls: ok (${files.length} files)`);
