// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { splitHighlights } from "../../search";
import { DOCUMENTS, searchFixtures } from "./documents";

describe("searchFixtures", () => {
	it("has 30 documents", () => {
		expect(DOCUMENTS).toHaveLength(30);
	});

	it("highlights exactly the query terms, Korean included", () => {
		const { hits } = searchFixtures("budget 예산");
		expect(hits.length).toBeGreaterThan(3);
		for (const hit of hits) {
			const marked = splitHighlights(hit.snippet, hit.highlights).filter(
				(s) => s.match,
			);
			for (const s of marked)
				expect(["budget", "예산"]).toContain(s.text.toLowerCase());
		}
		expect(
			hits.some((h) => h.highlights.length > 0 && h.snippet.includes("예산")),
		).toBe(true);
	});

	it("turns operators into filters, not search terms", () => {
		const r = searchFixtures("budget type:pdf after:2026-01");
		expect(r.filters).toEqual([
			{ key: "type", value: "pdf" },
			{ key: "after", value: "2026-01" },
		]);
		expect(
			r.hits.every(
				(h) =>
					h.highlights.length > 0 ||
					h.fileName.toLowerCase().includes("budget"),
			),
		).toBe(true);
	});

	it("finds nothing for an unknown word", () => {
		expect(searchFixtures("zebra").hits).toEqual([]);
	});
});
