// SPDX-License-Identifier: GPL-3.0-or-later
// Writes THIRD_PARTY_NOTICES for a release: every Rust crate in the built graph (cargo-about, offline,
// src-tauri/about.toml), the npm packages bundled into dist/ (production dependencies), PDFium, and the
// embedding models in the catalog (models/licenses/). The app compiles in the copy at the repository root.
// Usage: node scripts/third-party-notices.mjs <out-file>. Needs scripts/fetch-pdfium.sh and fetched crates.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const out = process.argv[2];
if (!out) throw new Error("usage: third-party-notices.mjs <out-file>");
const root = new URL("..", import.meta.url).pathname;
const tauri = join(root, "src-tauri");
const read = (p) => readFileSync(p, "utf8").trimEnd();

// about.toml's accepted list must match deny.toml's allow list, so a newly allowed license never fails a release.
const list = (file, key) => {
  const m = read(join(tauri, file)).match(new RegExp(`^${key}\\s*=\\s*\\[([\\s\\S]*?)\\]`, "m"));
  if (!m) throw new Error(`third-party-notices: no ${key} list in ${file}`);
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]).sort().join("\n");
};
if (list("deny.toml", "allow") !== list("about.toml", "accepted")) {
  throw new Error("third-party-notices: src-tauri/about.toml accepted differs from deny.toml allow");
}

const rule = `${"=".repeat(80)}\n`;
const parts = [];

parts.push(`${rule}Rust crates\n${rule}`);
// `cargo about --frozen` reads `cargo metadata` for every platform, but a build only downloads the crates
// of its own target, so a fresh machine lacks the rest. Fetch every locked crate first.
execFileSync("cargo", ["fetch", "--locked"], { cwd: tauri, stdio: "inherit" });
parts.push(
  execFileSync("cargo", ["about", "generate", "--frozen", "--fail", "about.hbs"], {
    cwd: tauri,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  }).trimEnd(),
);

parts.push(`\n${rule}npm packages bundled into the app's web UI\n${rule}`);
const npm = JSON.parse(
  execFileSync("pnpm", ["licenses", "list", "--prod", "--json"], { cwd: root, encoding: "utf8" }),
);
const pkgs = Object.values(npm)
  .flat()
  .sort((a, b) => a.name.localeCompare(b.name));
for (const p of pkgs) {
  for (const dir of p.paths) {
    // A package that ships no license file gets the upstream text from scripts/npm-licenses/<name>.txt.
    const files = readdirSync(dir)
      .filter((f) => /^(licen[cs]e|copying|notice)/i.test(f))
      .map((f) => join(dir, f));
    const fallback = join(root, "scripts", "npm-licenses", `${p.name}.txt`);
    if (files.length === 0 && existsSync(fallback)) files.push(fallback);
    if (files.length === 0) throw new Error(`third-party-notices: no license file in ${p.name}`);
    parts.push(`${"-".repeat(80)}\n${p.name} ${p.versions.join(", ")} (${p.license})\n`);
    for (const f of files.sort()) parts.push(`${read(f)}\n`);
  }
}

parts.push(`\n${rule}PDFium (libpdfium.dylib)\n${rule}`);
const pdfium = join(tauri, "vendor", "pdfium");
parts.push(`${read(join(pdfium, "LICENSE"))}\n`);
for (const f of readdirSync(join(pdfium, "licenses")).sort()) {
  parts.push(`${"-".repeat(80)}\n${f}\n\n${read(join(pdfium, "licenses", f))}\n`);
}

parts.push(`\n${rule}Embedding models (downloaded or imported separately, not bundled)\n${rule}`);
const catalog = JSON.parse(read(join(root, "models", "catalog.json")));
for (const m of catalog.models.filter((m) => m.kind === "embedding").sort((a, b) => a.id.localeCompare(b.id))) {
  parts.push(`${"-".repeat(80)}\n${read(join(root, "models", "licenses", `${m.id}.txt`))}\n`);
}

writeFileSync(out, `${parts.join("\n")}\n`);
console.log(`third-party-notices: wrote ${out} (${pkgs.length} npm packages)`);
