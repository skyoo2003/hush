// SPDX-License-Identifier: GPL-3.0-or-later
import type { Current } from "./ipc/Current";
import type { Reason } from "./ipc/Reason";
import type { SchedState } from "./ipc/SchedState";

// Must match `ingest::scheduler::SCHEDULE_EVENT`.
export const SCHEDULE_EVENT = "index://schedule";

const STATES: Record<SchedState, string> = {
	paused: "Paused",
	run_now: "Running now",
	low_power: "Low power",
	interactive: "Interactive",
	idle: "Idle",
	normal: "Normal",
};

const REASONS: Record<Reason, string> = {
	user_paused: "You paused indexing.",
	user_run_now: "You chose to index everything now.",
	outside_hours: "Outside the allowed hours.",
	thermal: "Your Mac is running hot.",
	low_power_mode: "Low Power Mode is on.",
	low_battery: "Battery is at 20% or less.",
	searching: "Search is open, so indexing yields to it.",
	user_idle: "You have been away for 5 minutes, so indexing speeds up.",
	default: "Indexing at a moderate pace.",
};

export function scheduleText(c: Current | null): string {
	if (!c) return "Schedule starts once the embedding model loads.";
	const { parseWorkers, embedding } = c.plan;
	const pace =
		parseWorkers === 0
			? "nothing runs"
			: `${parseWorkers} ${parseWorkers === 1 ? "worker" : "workers"}${embedding ? "" : ", embedding waits"}`;
	return `${STATES[c.state]}: ${REASONS[c.reason]} (${pace})`;
}

/** Minutes since midnight to the `HH:MM` an `<input type="time">` uses. */
export function toTime(minute: number): string {
	const pad = (n: number) => String(n).padStart(2, "0");
	return `${pad(Math.floor(minute / 60))}:${pad(minute % 60)}`;
}

/** `HH:MM` to minutes since midnight, or null for an empty or malformed value. */
export function fromTime(value: string): number | null {
	const m = /^(\d{2}):(\d{2})$/.exec(value);
	if (!m) return null;
	const [h, min] = [Number(m[1]), Number(m[2])];
	return h < 24 && min < 60 ? h * 60 + min : null;
}
