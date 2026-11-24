// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import type { ModelInfo } from "./ipc/ModelInfo";
import { formatBytes, importText, missingEmbedding, modelErrorText, percent } from "./models";

const file = (name: string, installed: boolean) => ({
	name,
	url: `https://huggingface.co/x/${name}`,
	size: 1,
	sha256: "0".repeat(64),
	installed,
});

const model = (id: string, kind: ModelInfo["kind"], installed: boolean): ModelInfo => ({
	id,
	kind,
	license: "MIT",
	redistributable: true,
	size: 3,
	installed,
	downloading: false,
	files: [file("config.json", installed), file("model.safetensors", installed)],
});

describe("models", () => {
	it("formats sizes in decimal units", () => {
		expect(formatBytes(743)).toBe("743 B");
		expect(formatBytes(711_396)).toBe("711 KB");
		expect(formatBytes(133_466_304)).toBe("133 MB");
		expect(formatBytes(2_019_377_696)).toBe("2.0 GB");
	});

	it("rounds progress down so 100% means done", () => {
		expect(percent(999, 1000)).toBe(99);
		expect(percent(1000, 1000)).toBe(100);
		expect(percent(0, 0)).toBe(0);
	});

	it("offers the first embedding model until any embedding model is installed", () => {
		const a = model("a", "embedding", false);
		const b = model("b", "embedding", false);
		const llm = model("l", "llm", true);
		expect(missingEmbedding([llm, a, b])?.id).toBe("a");
		expect(missingEmbedding([a, { ...b, installed: true }])).toBeNull();
		expect(missingEmbedding([])).toBeNull();
	});

	it("explains why a download was refused", () => {
		expect(modelErrorText({ code: "air_gap_enabled", message: "" })).toMatch(/Air-Gap/);
		expect(modelErrorText({ code: "hash_mismatch", message: "" })).toMatch(/SHA-256/);
		expect(modelErrorText(new Error("x"))).toMatch(/went wrong/);
	});

	it("lists the files an import still needs", () => {
		const m = model("a", "embedding", false);
		expect(importText({ id: "a", name: "config.json", modelInstalled: false }, [m])).toBe(
			"Imported config.json for a. Still needed: model.safetensors.",
		);
		expect(importText({ id: "a", name: "config.json", modelInstalled: true }, [m])).toBe(
			"a is installed.",
		);
	});
});
