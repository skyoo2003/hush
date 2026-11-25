// SPDX-License-Identifier: GPL-3.0-or-later
// 30 synthetic documents for the UI harness. Invented text only: no real content, names, or paths.
import type { QueryFilter } from "../../ipc/QueryFilter";
import type { SearchHit } from "../../ipc/SearchHit";
import type { SearchResults } from "../../ipc/SearchResults";

type Doc = Omit<SearchHit, "chunkId" | "highlights" | "score" | "matchKind">;

// 2026-01-05 .. 2026-11-20, fixed so dates render the same on every run.
const day = (d: number) => Date.UTC(2026, 0, 5) + d * 86_400_000;

// [fileName, relPath dir, folderLabel, page, sectionPath, snippet, day offset]
const ROWS: Array<
	[string, string, string, number | null, string | null, string, number]
> = [
	[
		"2026 budget plan.pdf",
		"Planning",
		"Work",
		4,
		null,
		"The 2026 budget allocates 18% more to field research and freezes travel spending until the second quarter review.",
		10,
	],
	[
		"2026 예산안.pdf",
		"기획",
		"Work",
		2,
		null,
		"2026년 예산은 현장 조사에 18% 더 배정하고, 2분기 검토 전까지 출장비를 동결한다.",
		12,
	],
	[
		"Q3 budget review.docx",
		"Planning",
		"Work",
		null,
		"Summary › Variance",
		"Q3 spending came in 4% under budget, mostly from deferred hardware purchases.",
		270,
	],
	[
		"Vendor contract draft.docx",
		"Legal",
		"Work",
		null,
		"Section 7 › Termination",
		"Either party may end this agreement with 60 days of written notice; the budget for exit costs is capped.",
		140,
	],
	[
		"임대차 계약서.pdf",
		"법무",
		"Work",
		3,
		null,
		"임차인은 계약 종료 30일 전까지 서면으로 갱신 여부를 통보해야 한다.",
		88,
	],
	[
		"Meeting notes 2026-03-14.md",
		"Notes",
		"Work",
		null,
		"Decisions",
		"We agreed to move the launch to May and revisit the budget once the pilot numbers land.",
		68,
	],
	[
		"회의록 2026-04-02.md",
		"노트",
		"Work",
		null,
		"결정 사항",
		"파일럿 결과를 본 뒤 예산을 다시 논의하기로 했다. 출시는 5월로 미룬다.",
		87,
	],
	[
		"Onboarding checklist.txt",
		"HR",
		"Work",
		null,
		null,
		"Day one: laptop setup, security training, and a walkthrough of the expense policy.",
		30,
	],
	[
		"Expense policy.pdf",
		"HR",
		"Work",
		9,
		null,
		"Receipts over 50,000 KRW must be attached within 14 days or the expense is not reimbursed.",
		45,
	],
	[
		"Field survey results.pdf",
		"Studies",
		"Research",
		22,
		null,
		"Respondents in rural areas reported 2.3× longer travel times to the nearest clinic.",
		150,
	],
	[
		"Literature review.docx",
		"Studies",
		"Research",
		null,
		"2 › Prior work",
		"Earlier studies measured access by distance alone and ignored public transport schedules.",
		160,
	],
	[
		"Interview transcript 07.txt",
		"Interviews",
		"Research",
		null,
		null,
		"She said the bus runs twice a day, so a clinic visit takes the whole afternoon.",
		172,
	],
	[
		"인터뷰 녹취 12.txt",
		"인터뷰",
		"Research",
		null,
		null,
		"버스가 하루에 두 번뿐이라 병원에 한 번 가려면 오후를 통째로 비워야 한다고 했다.",
		175,
	],
	[
		"Sampling method.md",
		"Methods",
		"Research",
		null,
		"Stratification",
		"We stratified villages by population and distance, then drew 40 households from each stratum.",
		120,
	],
	[
		"Grant proposal v3.pdf",
		"Grants",
		"Research",
		6,
		null,
		"The requested budget covers two field seasons, translation, and an independent audit.",
		200,
	],
	[
		"연구 계획서.docx",
		"과제",
		"Research",
		null,
		"3 › 연구 방법",
		"층화 표본 추출로 마을 40곳을 고르고, 가구별로 이동 시간을 기록한다.",
		205,
	],
	[
		"Whiteboard photo.png",
		"Scans",
		"Research",
		null,
		null,
		"Phase 1: survey · Phase 2: interviews · Phase 3: report (OCR text from a whiteboard photo)",
		180,
	],
	[
		"Receipt 2026-02-11.jpg",
		"2026",
		"Receipts",
		null,
		null,
		"Stationery store · Total 23,400 KRW · Card ending 0000",
		37,
	],
	[
		"영수증 2026-05-03.jpg",
		"2026",
		"Receipts",
		null,
		null,
		"문구점 · 합계 12,800원 · 카드 결제",
		118,
	],
	[
		"Tax summary 2025.pdf",
		"Tax",
		"Receipts",
		1,
		null,
		"Deductible expenses for 2025 total 1,240,000 KRW across 37 receipts.",
		25,
	],
	[
		"Insurance claim form.pdf",
		"Insurance",
		"Receipts",
		2,
		null,
		"Attach the original receipt and a copy of the diagnosis; claims are reviewed within 10 business days.",
		95,
	],
	[
		"Reading list.md",
		"Personal",
		"Archive",
		null,
		null,
		"Books on urban planning, transit design, and the history of public budgets.",
		5,
	],
	[
		"Old roadmap 2024.docx",
		"Plans",
		"Archive",
		null,
		"Milestones",
		"This roadmap was replaced in 2025; the budget figures here are out of date.",
		0,
	],
	[
		"Scanned letter.pdf",
		"Letters",
		"Archive",
		1,
		null,
		"Thank you for your letter of March 3. We have forwarded your request to the planning office.",
		60,
	],
	[
		"아이디어 메모.txt",
		"메모",
		"Archive",
		null,
		null,
		"검색 결과마다 출처 페이지를 붙이면 회의 자료를 만들 때 훨씬 빠를 것 같다.",
		300,
	],
	[
		"Conference talk outline.md",
		"Talks",
		"Archive",
		null,
		"Part 2",
		"Show the travel-time map first, then explain why distance alone misleads.",
		210,
	],
	[
		"Release notes draft.md",
		"Drafts",
		"Work",
		null,
		"Fixed",
		"Search now finds text in scanned PDFs, and results show the page they came from.",
		310,
	],
	[
		"Project charter.pdf",
		"Planning",
		"Work",
		1,
		null,
		"Scope: a local-first index of team documents. Out of scope: cloud sync and sharing.",
		3,
	],
	[
		"Risk register.docx",
		"Planning",
		"Work",
		null,
		"Open risks",
		"R4: the budget assumes the pilot finishes on time; a two-month slip adds 9% cost.",
		100,
	],
	[
		"Very long file name for a quarterly operating budget reconciliation spreadsheet export.pdf",
		"Finance/Reconciliation/2026/Quarterly exports",
		"Work",
		112,
		null,
		"Reconciled totals match the ledger except for three transfers booked in the following month.",
		280,
	],
];

export const DOCUMENTS: Doc[] = ROWS.map(
	([fileName, dir, folderLabel, page, sectionPath, snippet, d], i) => ({
		docId: i + 1,
		fileName,
		relPath: `${dir}/${fileName}`,
		folderLabel,
		page,
		sectionPath,
		snippet,
		modified: day(d),
	}),
);

const FILTER = /^(type|ext|folder|after|before):(\S+)$/;

/** Every case-insensitive occurrence of each term, as UTF-16 `[start, end)`. */
function highlights(text: string, terms: string[]): Array<[number, number]> {
	const lower = text.toLowerCase();
	const out: Array<[number, number]> = [];
	for (const term of terms) {
		for (
			let at = lower.indexOf(term);
			at !== -1;
			at = lower.indexOf(term, at + term.length)
		) {
			out.push([at, at + term.length]);
		}
	}
	return out;
}

/** A stand-in for the Rust search: keyword match over the fixtures, operators echoed as chips. */
export function searchFixtures(query: string): SearchResults {
	const tokens = query.trim().split(/\s+/).filter(Boolean);
	const filters = tokens.flatMap((t): QueryFilter[] => {
		const m = FILTER.exec(t);
		return m ? [{ key: m[1], value: m[2] } as QueryFilter] : [];
	});
	const terms = tokens
		.filter((t) => !FILTER.test(t))
		.map((t) => t.toLowerCase());
	const hits = DOCUMENTS.flatMap((doc): SearchHit[] => {
		const marks = highlights(doc.snippet, terms);
		if (
			marks.length === 0 &&
			!terms.some((t) => doc.fileName.toLowerCase().includes(t))
		)
			return [];
		return [
			{
				...doc,
				chunkId: doc.docId * 10,
				highlights: marks,
				score: 0,
				matchKind: marks.length ? "both" : "vector",
			},
		];
	}).map((h, i, all) => ({ ...h, score: 1 - i / (all.length + 1) }));
	return {
		hits,
		topScore: hits.length ? 0.82 : 0,
		scores: hits.map((h) => h.score),
		filters,
	};
}
