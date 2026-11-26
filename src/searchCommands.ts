// SPDX-License-Identifier: GPL-3.0-or-later
// The search and source commands, shared by quick search and the library window.
import { invoke } from "@tauri-apps/api/core";
import type { MessageKey } from "./i18n";
import type { AppError } from "./ipc/AppError";
import type { OpenMode } from "./ipc/OpenMode";
import type { SearchHit } from "./ipc/SearchHit";
import type { SearchResults } from "./ipc/SearchResults";

export function searchErrorKey(e: unknown): MessageKey {
	switch ((e as Partial<AppError>)?.code) {
		case "model_missing":
			return "search.modelMissing";
		case "source_not_found":
			return "search.sourceNotFound";
		case "clipboard_failed":
			return "search.clipboardFailed";
		default:
			return "search.failed";
	}
}

export const runSearch = (query: string) =>
	invoke<SearchResults>("search", {
		query,
		utcOffsetMinutes: -new Date().getTimezoneOffset(),
	});

/** Opens, reveals, or cites a hit. Rejects with the AppError. */
export function actOnHit(cmd: "open" | "reveal" | "copy", hit: SearchHit) {
	if (cmd === "copy") return invoke("copy_citation", { chunkId: hit.chunkId });
	const mode: OpenMode = cmd;
	return invoke("open_source", { chunkId: hit.chunkId, mode });
}
