// SPDX-License-Identifier: GPL-3.0-or-later
import type { SearchHit } from "./ipc/SearchHit";

export type Segment = { text: string; match: boolean };

/** Cuts `snippet` at `highlights` (UTF-16 `[start, end)`, the unit JS strings index by). */
export function splitHighlights(
  snippet: string,
  highlights: Array<[number, number]>,
): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  for (const [start, end] of [...highlights].sort((a, b) => a[0] - b[0])) {
    if (start < at || end <= start || end > snippet.length) continue;
    if (start > at) out.push({ text: snippet.slice(at, start), match: false });
    out.push({ text: snippet.slice(start, end), match: true });
    at = end;
  }
  if (at < snippet.length) out.push({ text: snippet.slice(at), match: false });
  return out;
}

/** "excerpt" — folder/path, p. N, section */
export function formatCitation(hit: SearchHit): string {
  const excerpt = hit.snippet.replace(/\s+/g, " ").trim();
  const where = [`${hit.folderLabel}/${hit.relPath}`];
  if (hit.page !== null) where.push(`p. ${hit.page}`);
  if (hit.sectionPath) where.push(hit.sectionPath);
  return `"${excerpt}" — ${where.join(", ")}`;
}
