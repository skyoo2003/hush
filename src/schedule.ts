// SPDX-License-Identifier: GPL-3.0-or-later
import { t } from "./i18n";
import type { Current } from "./ipc/Current";

// Must match `ingest::scheduler::SCHEDULE_EVENT`.
export const SCHEDULE_EVENT = "index://schedule";

export function scheduleText(c: Current | null): string {
	if (!c) return t("schedule.waiting");
	const { parseWorkers, embedding } = c.plan;
	const pace =
		parseWorkers === 0
			? t("schedule.pace.none")
			: parseWorkers === 1
				? t(embedding ? "schedule.pace.worker" : "schedule.pace.workerWaiting")
				: t(
						embedding
							? "schedule.pace.workers"
							: "schedule.pace.workersWaiting",
						{ count: parseWorkers },
					);
	return t("schedule.line", {
		state: t(`schedule.state.${c.state}`),
		reason: t(`schedule.reason.${c.reason}`),
		pace,
	});
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
