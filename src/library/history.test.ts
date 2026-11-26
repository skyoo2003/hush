// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { current, forgetFolder, go, push, routeFromKey, routeKey, START } from "./history";

describe("history", () => {
	it("goes back and forward, and a new route drops the forward entries", () => {
		let h = push(START, { kind: "ask" });
		h = push(h, { kind: "folder", id: 3 });
		expect(current(go(h, -1))).toEqual({ kind: "ask" });
		h = push(go(go(h, -1), -1), { kind: "compare" });
		expect(h.stack.map(routeKey)).toEqual(["search", "compare"]);
		expect(go(h, 1)).toBe(h);
		expect(go(go(h, -1), -1)).toEqual(go(h, -1));
	});

	it("ignores a push of the current route", () => {
		const h = push(START, { kind: "folder", id: 2 });
		expect(push(h, { kind: "folder", id: 2 })).toBe(h);
		expect(push(h, { kind: "folder", id: 4 }).index).toBe(2);
	});

	it("forgets a removed folder and keeps the place", () => {
		let h = push(START, { kind: "folder", id: 7 });
		h = push(h, { kind: "ask" });
		h = push(h, { kind: "folder", id: 7 });
		h = go(h, -1);
		const f = forgetFolder(h, 7);
		expect(f.stack.map(routeKey)).toEqual(["search", "ask"]);
		expect(current(f)).toEqual({ kind: "ask" });
		expect(forgetFolder(push(START, { kind: "folder", id: 1 }), 1).stack.map(routeKey)).toEqual(["search"]);
	});

	it("round-trips sidebar keys", () => {
		for (const r of [{ kind: "search" }, { kind: "folder", id: 12 }] as const)
			expect(routeFromKey(routeKey(r))).toEqual(r);
		expect(routeFromKey("nope")).toBeNull();
	});
});
