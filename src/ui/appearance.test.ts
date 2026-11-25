// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { applyAppearance, onAccent } from "./appearance";

function fakeRoot() {
	const props = new Map<string, string>();
	return {
		dataset: {} as DOMStringMap,
		style: {
			setProperty: (k: string, v: string) => props.set(k, v),
			removeProperty: (k: string) => props.delete(k),
		},
		props,
	};
}

describe("appearance", () => {
	it("picks the text color that reads on the accent", () => {
		expect(onAccent("#007aff")).toBe("white");
		expect(onAccent("#8c8c8c")).toBe("white");
		expect(onAccent("#ffc600")).toBe("black");
		expect(onAccent("#62ba46")).toBe("black");
		expect(onAccent("not a color")).toBeNull();
	});

	it("uses the system accent only when the setting says so", () => {
		const root = fakeRoot();
		const el = root as unknown as HTMLElement;
		applyAppearance(
			{ theme: "dark", accent: "system", systemAccent: "#ffc600" },
			el,
		);
		expect(root.dataset.theme).toBe("dark");
		expect(root.props.get("--accent")).toBe("#ffc600");
		expect(root.props.get("--fg-on-accent")).toBe("var(--prim-black)");

		applyAppearance(
			{ theme: "dark", accent: "brand", systemAccent: "#ffc600" },
			el,
		);
		expect(root.props.size).toBe(0);
	});
});
