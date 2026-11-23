// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import type { SearchHit } from "./ipc/SearchHit";
import { formatCitation, splitHighlights } from "./search";

const hit = (over: Partial<SearchHit> = {}): SearchHit => ({
  chunkId: 1,
  docId: 1,
  fileName: "terms.pdf",
  relPath: "legal/terms.pdf",
  folderLabel: "Contracts",
  page: 3,
  sectionPath: "Termination > Notice",
  snippet: "Either party may\n  terminate.",
  highlights: [],
  score: 0,
  matchKind: "both",
  modified: null,
  ...over,
});

describe("formatCitation", () => {
  it("cites excerpt, path, page, and section", () => {
    expect(formatCitation(hit())).toBe(
      '"Either party may terminate." — Contracts/legal/terms.pdf, p. 3, Termination > Notice',
    );
  });

  it("leaves out a missing page and section", () => {
    expect(formatCitation(hit({ page: null, sectionPath: null }))).toBe(
      '"Either party may terminate." — Contracts/legal/terms.pdf',
    );
  });
});

describe("splitHighlights", () => {
  it("marks matches in order, whatever order they arrive in", () => {
    expect(
      splitHighlights("a cat sat", [
        [6, 9],
        [2, 5],
      ]),
    ).toEqual([
      { text: "a ", match: false },
      { text: "cat", match: true },
      { text: " ", match: false },
      { text: "sat", match: true },
    ]);
  });

  it("indexes by UTF-16 units, like the backend", () => {
    // "😀" is two UTF-16 units, so "문서" starts at 3.
    expect(splitHighlights("😀 문서", [[3, 5]])).toEqual([
      { text: "😀 ", match: false },
      { text: "문서", match: true },
    ]);
  });

  it("drops overlapping and out-of-range spans", () => {
    expect(
      splitHighlights("abcdef", [
        [0, 3],
        [2, 4],
        [5, 99],
      ]),
    ).toEqual([
      { text: "abc", match: true },
      { text: "def", match: false },
    ]);
  });
});
