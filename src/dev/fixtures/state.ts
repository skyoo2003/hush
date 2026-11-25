// SPDX-License-Identifier: GPL-3.0-or-later
// The default backend state the harness starts from: a set-up app with three folders, an installed
// model, and Air-Gap on. Scenarios change pieces of it.
import { APP_NAME, SPONSOR_URLS } from "../../brand";
import type { ModelInfo } from "../../ipc/ModelInfo";
import type { Fixture } from "../mockIpc";
import { DOCUMENTS, searchFixtures } from "./documents";

const count = (label: string) =>
	DOCUMENTS.filter((d) => d.folderLabel === label).length;

const file = (
	repo: string,
	name: string,
	size: number,
	installed: boolean,
) => ({
	name,
	url: `https://huggingface.co/${repo}/resolve/0000000/${name}`,
	size,
	sha256: "0".repeat(64),
	installed,
});

export function models(installed: boolean): ModelInfo[] {
	return [
		{
			id: "multilingual-e5-small",
			kind: "embedding",
			license: "MIT",
			redistributable: true,
			size: 487_725_000,
			installed,
			downloading: false,
			files: [
				file("intfloat/multilingual-e5-small", "config.json", 655, installed),
				file(
					"intfloat/multilingual-e5-small",
					"tokenizer.json",
					17_082_730,
					installed,
				),
				file(
					"intfloat/multilingual-e5-small",
					"model.safetensors",
					470_641_600,
					installed,
				),
			],
		},
		{
			id: "bge-small-en-v1.5",
			kind: "embedding",
			license: "MIT",
			redistributable: true,
			size: 134_178_443,
			installed: false,
			downloading: false,
			files: [
				file("BAAI/bge-small-en-v1.5", "config.json", 743, false),
				file("BAAI/bge-small-en-v1.5", "tokenizer.json", 711_396, false),
				file("BAAI/bge-small-en-v1.5", "model.safetensors", 133_466_304, false),
			],
		},
	];
}

// A fake QR: a few squares, enough to show where the real one goes.
const QR_SVG =
	'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 7 7"><path d="M0 0h3v3H0zM4 0h3v3H4zM0 4h3v3H0zM4 4h1v1H4zM6 6h1v1H6z"/></svg>';

export function baseFixture(): Fixture {
	return {
		settings: {
			airGap: true,
			shortcut: "alt+Space",
			shortcutRegistered: true,
			excludePatterns: "drafts/\n*.bak",
			supporter: false,
			theme: "system",
			sponsorBanner: false,
		},
		folders: [
			{
				id: 1,
				path: "/Users/demo/Documents/Work",
				label: "Work",
				status: "active",
				enabled: true,
				documents: count("Work"),
				failed: 2,
				skipped: 1,
			},
			{
				id: 2,
				path: "/Users/demo/Documents/Research",
				label: "Research",
				status: "active",
				enabled: true,
				documents: count("Research"),
				failed: 0,
				skipped: 0,
			},
			{
				id: 3,
				path: "/Users/demo/Documents/Receipts",
				label: "Receipts",
				status: "active",
				enabled: false,
				documents: count("Receipts"),
				failed: 0,
				skipped: 0,
			},
			{
				id: 4,
				path: "/Volumes/Backup/Archive",
				label: "Archive",
				status: "unavailable",
				enabled: true,
				documents: count("Archive"),
				failed: 0,
				skipped: 3,
			},
		],
		failed: {
			1: [
				{
					docId: 901,
					relPath: "Legal/Signed NDA (locked).pdf",
					reason: "encrypted",
				},
				{
					docId: 902,
					relPath: "Drafts/broken export.docx",
					reason: "corrupted",
				},
			],
		},
		index: {
			indexing: true,
			progress: {
				queued: 0,
				running: 0,
				done: DOCUMENTS.length,
				failed: 2,
				paused: false,
			},
		},
		schedule: {
			settings: { mode: "auto", allowedHours: null, lowPower: "pause" },
			current: {
				state: "idle",
				reason: "user_idle",
				plan: { parseWorkers: 2, embedding: true },
			},
		},
		models: models(true),
		about: {
			name: APP_NAME,
			version: "0.1.0",
			copyright: `Copyright © 2026 ${APP_NAME} contributors`,
			license: "GPL-3.0-or-later",
			licenseUrl: "https://www.gnu.org/licenses/gpl-3.0.html",
			sourceUrl: "https://example.invalid/source/tree/0000000",
		},
		legal:
			"GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007\n\n(License text placeholder for the UI harness.)\n",
		search: searchFixtures,
		sponsorQr: { kind: "qr", url: SPONSOR_URLS[0], svg: QR_SVG },
		update: { version: "0.2.0" },
		addResult: null,
		fail: {},
		hang: [],
		events: [],
	};
}
