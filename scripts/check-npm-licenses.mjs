// SPDX-License-Identifier: GPL-3.0-or-later
// Fails if a pnpm dependency's license is outside the allow list in src-tauri/deny.toml,
// so crates and npm packages share one GPL-3.0-compatible list.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const deny = readFileSync(new URL("../src-tauri/deny.toml", import.meta.url), "utf8");
const block = deny.match(/^allow\s*=\s*\[([\s\S]*?)\]/m);
if (!block) throw new Error("check-npm-licenses: no allow list in src-tauri/deny.toml");
const allow = new Set([...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]));

// shortcut: flattens parentheses, so "A AND (B OR C)" is read as "A AND B OR C"; parse properly if such a license appears.
const allowed = (expr) =>
  expr
    .replace(/[()]/g, "")
    .split(/\s+OR\s+/)
    .some((alt) => alt.split(/\s+AND\s+/).every((term) => allow.has(term.trim())));

const out = execFileSync("pnpm", ["licenses", "list", "--json"], { encoding: "utf8" });
const bad = Object.entries(JSON.parse(out))
  .filter(([license]) => !allowed(license))
  .flatMap(([license, pkgs]) => pkgs.map((p) => `${p.name}@${p.versions.join(",")}: ${license}`));

if (bad.length > 0) {
  console.error(bad.join("\n"));
  console.error("check-npm-licenses: licenses above are not in src-tauri/deny.toml's allow list");
  process.exit(1);
}
console.log("check-npm-licenses: ok");
