// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FolderPlus } from "lucide-react";
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
import { Group, Row } from "../settings/Form";
import { Alert } from "../ui/Alert";
import { Banner } from "../ui/Banner";
import { Button } from "../ui/Button";
import { Checkbox } from "../ui/Checkbox";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";

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

export function progressText(indexing: boolean, p: IndexProgress | null): string {
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

	const removing = folders.find((f) => f.id === confirming);
	return (
		<>
			<Group
				title={t("folders.heading")}
				description={
					<span role="status">
						{progressText(status?.indexing ?? false, status?.progress ?? null)}
					</span>
				}
			>
				{folders.length === 0 && <Row label={t("folders.empty")} />}
				{folders.map((f) => (
					<Row
						key={f.id}
						label={
							<>
								<strong>{f.label}</strong>
								<div className="form-mono" title={f.path} data-selectable>
									{f.path}
								</div>
								<div className="form-row-note">
									{t("folders.counts", {
										documents: f.documents,
										errors: f.failed,
										skipped: f.skipped,
									})}
									{!f.enabled && ` ${t("folders.pausedNote")}`}
									{f.status === "unavailable" && ` ${t("folders.unavailable")}`}
								</div>
							</>
						}
						note={
							errorsOf === f.id && (
								<ul
									className="form-list"
									aria-label={t("folders.errorsIn", { label: f.label })}
								>
									{failedDocs.map((d) => (
										<li key={d.docId}>
											<span className="form-mono" data-selectable>
												{d.relPath}
											</span>{" "}
											<em>{t(`folders.reason.${d.reason}`)}</em>{" "}
											<Button
												size="small"
												disabled={busy || !f.enabled}
												onClick={() => void retry(d)}
												aria-label={t("folders.retryLabel", { path: d.relPath })}
											>
												{t("folders.retry")}
											</Button>
										</li>
									))}
								</ul>
							)
						}
					>
						{f.failed > 0 && (
							<Button
								size="small"
								variant="plain"
								aria-expanded={errorsOf === f.id}
								onClick={() => setErrorsOf(errorsOf === f.id ? null : f.id)}
							>
								{errorsOf === f.id
									? t("folders.hideErrors")
									: t("folders.showErrors")}
							</Button>
						)}
						<Switch
							label={f.label}
							className="form-switch-only"
							checked={f.enabled}
							disabled={busy}
							onCheckedChange={(v) => void setEnabled(f, v)}
						/>
						<Button
							size="small"
							disabled={busy}
							onClick={() => setConfirming(f.id)}
							aria-label={t("folders.removeLabel", { label: f.label })}
						>
							{t("folders.removePrompt")}
						</Button>
					</Row>
				))}
				<Row>
					<Button icon={FolderPlus} disabled={busy} onClick={() => void add()}>
						{t("folders.add")}
					</Button>
				</Row>
			</Group>
			{notice && <Banner tone="info">{notice}</Banner>}
			{error && <Banner tone="error">{error}</Banner>}
			<Alert
				open={removing !== undefined}
				onOpenChange={(open) => !open && setConfirming(null)}
				title={t("folders.removeConfirm", { label: removing?.label ?? "" })}
				tone="destructive"
				confirmLabel={t("folders.remove")}
				cancelLabel={t("action.cancel")}
				onConfirm={() => removing && void remove(removing)}
			/>
			<Group
				title={t("folders.excludeLabel")}
				description={
					<span id="exclude-help">
						{t("folders.excludeHelp.lead")} <code>drafts/</code>{" "}
						{t("folders.excludeHelp.or")} <code>*.bak</code>
						{t("folders.excludeHelp.tail")}
					</span>
				}
			>
				<Row>
					<textarea
						id="exclude-patterns"
						className="form-textarea"
						aria-label={t("folders.excludeLabel")}
						aria-describedby="exclude-help"
						rows={4}
						value={patterns}
						onChange={(e) => setPatterns(e.currentTarget.value)}
					/>
				</Row>
				<Row>
					<Button
						disabled={busy || patterns === savedPatterns}
						onClick={() => void savePatterns()}
					>
						{t("folders.savePatterns")}
					</Button>
				</Row>
			</Group>
			<Group
				title={t("schedule.heading")}
				description={
					<span role="status" id="schedule-status">
						{scheduleText(schedule?.current ?? null)}
					</span>
				}
			>
				{sched && (
					<>
						<Row label={t("schedule.modeLegend")}>
							<SegmentedControl
								label={t("schedule.modeLegend")}
								disabled={busy}
								options={(["auto", "paused", "run_now"] as const).map(
									(mode) => ({ value: mode, label: t(`schedule.mode.${mode}`) }),
								)}
								value={sched.mode}
								onValueChange={(mode) => void saveSchedule({ ...sched, mode })}
							/>
						</Row>
						<Row
							label={
								<Checkbox
									label={t("schedule.onlyBetween")}
									disabled={busy}
									checked={sched.allowedHours !== null}
									onCheckedChange={(on) =>
										void saveSchedule({
											...sched,
											allowedHours:
												on === true
													? { startMinute: 22 * 60, endMinute: 7 * 60 }
													: null,
										})
									}
								/>
							}
						>
							{sched.allowedHours && (
								<>
									<input
										type="time"
										className="field-box textfield"
										disabled={busy}
										aria-label={t("schedule.startTime")}
										value={toTime(sched.allowedHours.startMinute)}
										onChange={(e) =>
											setHour("startMinute", e.currentTarget.value)
										}
									/>
									{t("schedule.and")}
									<input
										type="time"
										className="field-box textfield"
										disabled={busy}
										aria-label={t("schedule.endTime")}
										value={toTime(sched.allowedHours.endMinute)}
										onChange={(e) => setHour("endMinute", e.currentTarget.value)}
									/>
								</>
							)}
						</Row>
						<Row label={t("schedule.lowPowerLabel")}>
							<Select
								label={t("schedule.lowPowerLabel")}
								disabled={busy}
								value={sched.lowPower}
								options={(["pause", "one_worker"] as const).map((v) => ({
									value: v,
									label: t(`schedule.lowPower.${v}`),
								}))}
								onValueChange={(lowPower) =>
									void saveSchedule({ ...sched, lowPower })
								}
							/>
						</Row>
					</>
				)}
			</Group>
		</>
	);
}
