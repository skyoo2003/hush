// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import type { AddedFolder } from "../ipc/AddedFolder";
import type { AppError } from "../ipc/AppError";
import type { FailedDocument } from "../ipc/FailedDocument";
import type { FolderInfo } from "../ipc/FolderInfo";
import type { IndexProgress } from "../ipc/IndexProgress";
import type { IndexStatus } from "../ipc/IndexStatus";
import type { ScheduleSettings } from "../ipc/ScheduleSettings";
import type { ScheduleStatus } from "../ipc/ScheduleStatus";
import type { Settings } from "../ipc/Settings";
import { fromTime, SCHEDULE_EVENT, scheduleText, toTime } from "../schedule";

// Must match `ingest::queue::PROGRESS_EVENT`.
const PROGRESS_EVENT = "index://progress";

function errorText(e: unknown): string {
	switch ((e as Partial<AppError>)?.code) {
		case "folder_overlap":
			return t("folders.error.overlap");
		case "folder_not_found":
			return t("folders.error.notFound");
		case "folder_unreadable":
			return t("folders.error.unreadable", { app: APP_NAME });
		case "model_missing":
			return t("folders.error.modelMissing");
		case "schedule_invalid":
			return t("folders.error.scheduleInvalid");
		default:
			return t("error.generic");
	}
}

function progressText(indexing: boolean, p: IndexProgress | null): string {
	if (!indexing || !p) return t("folders.progress.waiting");
	if (p.paused) return t("folders.progress.paused");
	const pending = p.queued + p.running;
	if (!pending)
		return p.failed
			? t("folders.progress.upToDateFailed", { failed: p.failed })
			: t("folders.progress.upToDate");
	return p.failed
		? t("folders.progress.runningFailed", {
				pending,
				done: p.done,
				failed: p.failed,
			})
		: t("folders.progress.running", { pending, done: p.done });
}

export function FolderAndSchedulePanel() {
	const [folders, setFolders] = useState<FolderInfo[]>([]);
	const [status, setStatus] = useState<IndexStatus | null>(null);
	const [busy, setBusy] = useState(false);
	const [notice, setNotice] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [confirming, setConfirming] = useState<number | null>(null);
	// The folder whose error list is open, and that list.
	const [errorsOf, setErrorsOf] = useState<number | null>(null);
	const [failedDocs, setFailedDocs] = useState<FailedDocument[]>([]);
	const [patterns, setPatterns] = useState("");
	const [savedPatterns, setSavedPatterns] = useState("");
	const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);

	const refresh = useCallback(() => {
		invoke<FolderInfo[]>("list_folders").then(setFolders, () =>
			setError(t("folders.listFailed")),
		);
	}, []);

	// Reloads when the open folder's error count changes, e.g. after a retry succeeds.
	const openFailed = folders.find((f) => f.id === errorsOf)?.failed;
	useEffect(() => {
		if (errorsOf === null || openFailed === undefined) return;
		invoke<FailedDocument[]>("list_failed_documents", {
			folderId: errorsOf,
		}).then(setFailedDocs, () => setError(t("folders.errorListFailed")));
	}, [errorsOf, openFailed]);

	useEffect(() => {
		refresh();
		invoke<IndexStatus>("get_index_status").then(setStatus, () => {});
		invoke<Settings>("get_settings").then(
			(s) => {
				setPatterns(s.excludePatterns);
				setSavedPatterns(s.excludePatterns);
			},
			() => {},
		);
		const loadSchedule = () =>
			invoke<ScheduleStatus>("get_schedule").then(setSchedule, () => {});
		void loadSchedule();
		// Counts change as documents finish, so the list refreshes with each progress event.
		const unlisten = listen<IndexProgress>(PROGRESS_EVENT, (e) => {
			setStatus({ indexing: true, progress: e.payload });
			refresh();
		});
		// Re-read rather than use the payload: the tray's Pause also changes the settings.
		const unlistenSchedule = listen(SCHEDULE_EVENT, () => void loadSchedule());
		return () => {
			void unlisten.then((f) => f());
			void unlistenSchedule.then((f) => f());
		};
	}, [refresh]);

	const run = async (action: () => Promise<void>) => {
		setBusy(true);
		setError(null);
		setNotice(null);
		try {
			await action();
		} catch (e) {
			setError(errorText(e));
		} finally {
			setBusy(false);
			refresh();
		}
	};

	const add = () =>
		run(async () => {
			const added = await invoke<AddedFolder | null>("add_watched_folder");
			if (added?.warning === "broad_path") {
				setNotice(t("folders.broadPath"));
			}
		});

	const remove = (f: FolderInfo) =>
		run(async () => {
			setConfirming(null);
			if (errorsOf === f.id) setErrorsOf(null);
			await invoke("remove_watched_folder", { id: f.id });
		});

	const setEnabled = (f: FolderInfo, enabled: boolean) =>
		run(async () => {
			await invoke("set_folder_enabled", { id: f.id, enabled });
		});

	const retry = (d: FailedDocument) =>
		run(async () => {
			await invoke("retry_document", { docId: d.docId });
			setNotice(t("folders.retryQueued", { path: d.relPath }));
		});

	const savePatterns = () =>
		run(async () => {
			const s = await invoke<Settings>("set_exclude_patterns", { patterns });
			setPatterns(s.excludePatterns);
			setSavedPatterns(s.excludePatterns);
			setNotice(t("folders.patternsSaved"));
		});

	const saveSchedule = (next: ScheduleSettings) =>
		run(async () => {
			setSchedule(
				await invoke<ScheduleStatus>("set_schedule", { settings: next }),
			);
		});

	const sched = schedule?.settings;
	const setHour = (which: "startMinute" | "endMinute", value: string) => {
		const minute = fromTime(value);
		if (!sched?.allowedHours || minute === null) return;
		void saveSchedule({
			...sched,
			allowedHours: { ...sched.allowedHours, [which]: minute },
		});
	};

	return (
		<section aria-labelledby="folders-heading">
			<h2 id="folders-heading">{t("folders.heading")}</h2>
			<p role="status">
				{progressText(status?.indexing ?? false, status?.progress ?? null)}
			</p>
			{folders.length === 0 ? (
				<p>{t("folders.empty")}</p>
			) : (
				<ul>
					{folders.map((f) => (
						<li key={f.id}>
							<label>
								<input
									type="checkbox"
									checked={f.enabled}
									disabled={busy}
									onChange={(e) => void setEnabled(f, e.currentTarget.checked)}
								/>{" "}
								<strong>{f.label}</strong>
							</label>{" "}
							<small title={f.path}>{f.path}</small>{" "}
							<span>
								{t("folders.counts", {
									documents: f.documents,
									errors: f.failed,
									skipped: f.skipped,
								})}
								{!f.enabled && ` ${t("folders.pausedNote")}`}
								{f.status === "unavailable" && ` ${t("folders.unavailable")}`}
							</span>{" "}
							{f.failed > 0 && (
								<button
									type="button"
									aria-expanded={errorsOf === f.id}
									onClick={() => setErrorsOf(errorsOf === f.id ? null : f.id)}
								>
									{errorsOf === f.id
										? t("folders.hideErrors")
										: t("folders.showErrors")}
								</button>
							)}{" "}
							{confirming === f.id ? (
								<fieldset>
									<legend>
										{t("folders.removeConfirm", { label: f.label })}
									</legend>
									<button
										type="button"
										disabled={busy}
										onClick={() => void remove(f)}
									>
										{t("folders.remove")}
									</button>{" "}
									<button type="button" onClick={() => setConfirming(null)}>
										{t("action.cancel")}
									</button>
								</fieldset>
							) : (
								<button
									type="button"
									disabled={busy}
									onClick={() => setConfirming(f.id)}
									aria-label={t("folders.removeLabel", { label: f.label })}
								>
									{t("folders.removePrompt")}
								</button>
							)}
							{errorsOf === f.id && (
								<ul aria-label={t("folders.errorsIn", { label: f.label })}>
									{failedDocs.map((d) => (
										<li key={d.docId}>
											<span>{d.relPath}</span>{" "}
											<em>{t(`folders.reason.${d.reason}`)}</em>{" "}
											<button
												type="button"
												disabled={busy || !f.enabled}
												onClick={() => void retry(d)}
												aria-label={t("folders.retryLabel", { path: d.relPath })}
											>
												{t("folders.retry")}
											</button>
										</li>
									))}
								</ul>
							)}
						</li>
					))}
				</ul>
			)}
			<button type="button" disabled={busy} onClick={() => void add()}>
				{t("folders.add")}
			</button>
			<h3>
				<label htmlFor="exclude-patterns">{t("folders.excludeLabel")}</label>
			</h3>
			<p id="exclude-help">
				{t("folders.excludeHelp.lead")} <code>drafts/</code>{" "}
				{t("folders.excludeHelp.or")} <code>*.bak</code>
				{t("folders.excludeHelp.tail")}
			</p>
			<textarea
				id="exclude-patterns"
				aria-describedby="exclude-help"
				rows={4}
				value={patterns}
				onChange={(e) => setPatterns(e.currentTarget.value)}
			/>
			<button
				type="button"
				disabled={busy || patterns === savedPatterns}
				onClick={() => void savePatterns()}
			>
				{t("folders.savePatterns")}
			</button>
			<h3 id="schedule-heading">{t("schedule.heading")}</h3>
			<p role="status" aria-labelledby="schedule-heading">
				{scheduleText(schedule?.current ?? null)}
			</p>
			{sched && (
				<>
					<fieldset disabled={busy}>
						<legend>{t("schedule.modeLegend")}</legend>
						{(["auto", "paused", "run_now"] as const).map((mode) => (
							<label key={mode}>
								<input
									type="radio"
									name="schedule-mode"
									checked={sched.mode === mode}
									onChange={() => void saveSchedule({ ...sched, mode })}
								/>{" "}
								{t(`schedule.mode.${mode}`)}
							</label>
						))}
					</fieldset>
					<fieldset disabled={busy}>
						<legend>{t("schedule.hoursLegend")}</legend>
						<label>
							<input
								type="checkbox"
								checked={sched.allowedHours !== null}
								onChange={(e) =>
									void saveSchedule({
										...sched,
										allowedHours: e.currentTarget.checked
											? { startMinute: 22 * 60, endMinute: 7 * 60 }
											: null,
									})
								}
							/>{" "}
							{t("schedule.onlyBetween")}
						</label>{" "}
						{sched.allowedHours && (
							<>
								<input
									type="time"
									aria-label={t("schedule.startTime")}
									value={toTime(sched.allowedHours.startMinute)}
									onChange={(e) =>
										setHour("startMinute", e.currentTarget.value)
									}
								/>{" "}
								{t("schedule.and")}{" "}
								<input
									type="time"
									aria-label={t("schedule.endTime")}
									value={toTime(sched.allowedHours.endMinute)}
									onChange={(e) => setHour("endMinute", e.currentTarget.value)}
								/>
							</>
						)}
					</fieldset>
					<label>
						{t("schedule.lowPowerLabel")}{" "}
						<select
							disabled={busy}
							value={sched.lowPower}
							onChange={(e) =>
								void saveSchedule({
									...sched,
									lowPower: e.currentTarget
										.value as ScheduleSettings["lowPower"],
								})
							}
						>
							<option value="pause">{t("schedule.lowPower.pause")}</option>
							<option value="one_worker">
								{t("schedule.lowPower.one_worker")}
							</option>
						</select>
					</label>
				</>
			)}
			{notice && <p>{notice}</p>}
			{error && <p role="alert">{error}</p>}
		</section>
	);
}
