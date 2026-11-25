// SPDX-License-Identifier: GPL-3.0-or-later
import { APP_NAME } from "./brand";
import { t } from "./i18n";
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
			return t("models.error.airGapEnabled");
		case "hash_mismatch":
			return t("models.error.hashMismatch");
		case "not_in_catalog":
			return t("models.error.notInCatalog", { app: APP_NAME });
		case "download_cancelled":
			return t("models.error.downloadCancelled");
		case "download_failed":
			return t("models.error.downloadFailed");
		default:
			return t("error.generic");
	}
}

/** One sentence per model; a kit lists several files of each model, and the last one is current. */
export function importText(rs: ImportedFile[], models: ModelInfo[]): string {
	const last = new Map(rs.map((r) => [r.id, r]));
	return [...last.values()]
		.map((r) => {
			if (r.modelInstalled) return t("models.installed", { id: r.id });
			const got = rs.filter((x) => x.id === r.id).map((x) => x.name);
			const left = models
				.find((m) => m.id === r.id)
				?.files.filter((f) => !f.installed && !got.includes(f.name))
				.map((f) => f.name);
			const name = got.join(", ");
			return left?.length
				? t("models.importedNeeds", { name, id: r.id, left: left.join(", ") })
				: t("models.imported", { name, id: r.id });
		})
		.join(" ");
}
