// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { t } from "./i18n";
import { updateErrorText } from "./update";

describe("updateErrorText", () => {
	it("names a rejected signature and a missing key apart from other failures", () => {
		expect(updateErrorText({ code: "update_signature_invalid", message: "" })).toBe(
			t("update.error.signature"),
		);
		expect(updateErrorText({ code: "updater_not_configured", message: "" })).toBe(
			t("update.error.notConfigured"),
		);
		expect(updateErrorText({ code: "air_gap_enabled", message: "" })).toBe(
			t("update.error.airGapEnabled"),
		);
		expect(updateErrorText({ code: "update_failed", message: "" })).toBe(
			t("update.error.failed"),
		);
		expect(updateErrorText(new Error("x"))).toBe(t("update.error.failed"));
	});
});
