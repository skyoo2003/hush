// SPDX-License-Identifier: GPL-3.0-or-later
import { t } from "./i18n";
import type { AppError } from "./ipc/AppError";

export function updateErrorText(e: unknown): string {
	switch ((e as Partial<AppError>)?.code) {
		case "air_gap_enabled":
			return t("update.error.airGapEnabled");
		case "updater_not_configured":
			return t("update.error.notConfigured");
		case "update_signature_invalid":
			return t("update.error.signature");
		default:
			return t("update.error.failed");
	}
}
