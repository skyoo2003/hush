// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { contrast, mix, over, parseHex } from "./contrast";

const hex = (h: string) => {
	const c = parseHex(h);
	if (!c) throw new Error(h);
	return c;
};

describe("contrast", () => {
	it("matches the WCAG reference values", () => {
		expect(contrast(hex("#000"), hex("#fff"))).toBeCloseTo(21, 5);
		expect(contrast(hex("#777777"), hex("#ffffff"))).toBeCloseTo(4.48, 2);
		expect(contrast(hex("#fff"), hex("#fff"))).toBe(1);
	});

	it("mixes like color-mix(in srgb) and composites alpha", () => {
		const transparent = { r: 0, g: 0, b: 0, a: 0 };
		// Mixing with transparent keeps the color and scales alpha (premultiplied).
		expect(mix(hex("#ff0000"), 0.25, transparent)).toEqual({
			r: 1,
			g: 0,
			b: 0,
			a: 0.25,
		});
		expect(mix(hex("#000"), 0.5, hex("#fff")).r).toBeCloseTo(0.5);
		expect(over({ r: 0, g: 0, b: 0, a: 0.5 }, hex("#fff")).g).toBeCloseTo(0.5);
	});

	it("rejects what is not a hex color", () => {
		expect(parseHex("rgb(0 0 0)")).toBeNull();
		expect(parseHex("#12345")).toBeNull();
	});
});
