// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { progressText } from "../components/FolderAndSchedulePanel";
import { t } from "../i18n";
import type { IndexStatus } from "../ipc/IndexStatus";
import type { ScheduleStatus } from "../ipc/ScheduleStatus";
import { SCHEDULE_EVENT, scheduleText } from "../schedule";
import { Button } from "../ui/Button";
import { Popover } from "../ui/Popover";
import { ProgressRing } from "../ui/ProgressRing";

/** The tray's rule: finished is done + failed, of everything known to the queue. */
export function footerText(status: IndexStatus | null): {
	label: string;
	fraction: number | undefined;
} {
	if (!status?.indexing) return { label: t("footer.waiting"), fraction: undefined };
	const p = status.progress;
	const done = p.done + p.failed;
	const total = done + p.queued + p.running;
	if (total === done) return { label: t("footer.upToDate"), fraction: 1 };
	const label = t(p.paused ? "footer.paused" : "footer.indexing", { done, total });
	return { label, fraction: done / total };
}

/** Sidebar footer: progress ring and count; the popover adds the schedule and a way to Settings. */
export function IndexFooter({ status }: { status: IndexStatus | null }) {
	const [open, setOpen] = useState(false);
	const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);

	useEffect(() => {
		if (!open) return;
		const load = () =>
			invoke<ScheduleStatus>("get_schedule").then(setSchedule, () => {});
		void load();
		const unlisten = listen(SCHEDULE_EVENT, () => void load());
		return () => {
			void unlisten.then((f) => f());
		};
	}, [open]);

	const { label, fraction } = footerText(status);
	return (
		<Popover
			open={open}
			onOpenChange={setOpen}
			side="top"
			align="start"
			trigger={
				<button
					type="button"
					className="index-footer"
					aria-label={`${label}. ${t("footer.details")}`}
				>
					<ProgressRing label={label} value={fraction} />
					<span>{label}</span>
				</button>
			}
		>
			<div className="index-footer-details">
				<p>{progressText(status?.indexing ?? false, status?.progress ?? null)}</p>
				<p>{scheduleText(schedule?.current ?? null)}</p>
				<Button
					size="small"
					onClick={() => {
						setOpen(false);
						void invoke("open_settings", { pane: "folders" });
					}}
				>
					{t("footer.openSettings")}
				</Button>
			</div>
		</Popover>
	);
}

