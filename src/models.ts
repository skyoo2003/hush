// SPDX-License-Identifier: GPL-3.0-or-later
import { APP_NAME } from "./brand";
import type { AppError } from "./ipc/AppError";
import type { ImportedFile } from "./ipc/ImportedFile";
import type { ModelInfo } from "./ipc/ModelInfo";

// Must match `rag::models::PROGRESS_EVENT`.
export const MODEL_PROGRESS_EVENT = "models://progress";

/** Decimal units, like Finder. */
export function formatBytes(n: number): string {
	const units = ["B", "KB", "MB", "GB"];
	let i = 0;
	while (n >= 1000 && i < units.length - 1) {
		n /= 1000;
		i++;
	}
	return `${i === 0 || n >= 10 ? Math.round(n) : n.toFixed(1)} ${units[i]}`;
}

export function percent(done: number, total: number): number {
	return total > 0 ? Math.floor((done / total) * 100) : 0;
}

/** The embedding model to offer on first run, or null once one is installed. */
export function missingEmbedding(models: ModelInfo[]): ModelInfo | null {
	const embedding = models.filter((m) => m.kind === "embedding");
	return embedding.some((m) => m.installed) ? null : (embedding[0] ?? null);
}

export function modelErrorText(e: unknown): string {
	switch ((e as Partial<AppError>)?.code) {
		case "air_gap_enabled":
			return "Air-Gap mode is on, so nothing may be downloaded. Turn it off, or import the file instead.";
		case "hash_mismatch":
			return "The file does not match the expected SHA-256 checksum, so it was not installed. Download it again.";
		case "not_in_catalog":
			return `That file is not one of the models ${APP_NAME} knows.`;
		case "download_cancelled":
			return "Download cancelled. Downloading again continues where it stopped.";
		case "download_failed":
			return "The download failed. Downloading again continues where it stopped.";
		default:
			return "Something went wrong. Try again.";
	}
}

export function importText(r: ImportedFile, models: ModelInfo[]): string {
	if (r.modelInstalled) return `${r.id} is installed.`;
	const left = models
		.find((m) => m.id === r.id)
		?.files.filter((f) => !f.installed && f.name !== r.name)
		.map((f) => f.name);
	return `Imported ${r.name} for ${r.id}.${left?.length ? ` Still needed: ${left.join(", ")}.` : ""}`;
}
