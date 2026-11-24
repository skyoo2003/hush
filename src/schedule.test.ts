// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { fromTime, scheduleText, toTime } from "./schedule";

describe("scheduleText", () => {
	it("waits for the model", () => {
		expect(scheduleText(null)).toMatch(/model loads/);
	});
	it("names state, reason, and pace", () => {
		expect(
			scheduleText({
				state: "interactive",
				reason: "searching",
				plan: { parseWorkers: 1, embedding: false },
			}),
		).toBe(
			"Interactive: Search is open, so indexing yields to it. (1 worker, embedding waits)",
		);
		expect(
			scheduleText({
				state: "paused",
				reason: "outside_hours",
				plan: { parseWorkers: 0, embedding: false },
			}),
		).toBe("Paused: Outside the allowed hours. (nothing runs)");
	});
});

describe("time conversion", () => {
	it("round-trips", () => {
		for (const m of [0, 7 * 60, 22 * 60 + 30, 1439])
			expect(fromTime(toTime(m))).toBe(m);
	});
	it("rejects bad input", () => {
		for (const v of ["", "7:00", "24:00", "12:60", "ab:cd"])
			expect(fromTime(v)).toBeNull();
	});
});
