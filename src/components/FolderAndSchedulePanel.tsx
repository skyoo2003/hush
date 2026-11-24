// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import type { AddedFolder } from "../ipc/AddedFolder";
import type { AppError } from "../ipc/AppError";
import type { FailReason } from "../ipc/FailReason";
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
			return "That folder is already watched, or is inside or contains a watched folder.";
		case "folder_not_found":
			return "That folder no longer exists.";
		case "folder_unreadable":
			return `${APP_NAME} cannot read that folder. Check its permissions.`;
		case "model_missing":
			return "Indexing is off until the embedding model is installed.";
		case "schedule_invalid":
			return "Pick two different times for the allowed hours.";
		default:
			return "Something went wrong. Try again.";
	}
}

const REASONS: Record<FailReason, string> = {
	encrypted: "Encrypted PDF",
	corrupted: "Corrupted",
	too_large: "Too large",
	ocr_failed: "OCR failed",
	read_failed: "Could not be read",
	embed_failed: "Embedding failed",
	parse_failed: "Could not be parsed",
};

function progressText(indexing: boolean, p: IndexProgress | null): string {
	if (!indexing || !p) return "Indexing starts once the embedding model loads.";
	if (p.paused) return "Indexing paused.";
	const pending = p.queued + p.running;
	const failed = p.failed ? `, ${p.failed} failed` : "";
	return pending
		? `Indexing: ${pending} left, ${p.done} done${failed}.`
		: `Up to date${failed}.`;
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
			setError("Could not read the folder list."),
		);
	}, []);

	// Reloads when the open folder's error count changes, e.g. after a retry succeeds.
	const openFailed = folders.find((f) => f.id === errorsOf)?.failed;
	useEffect(() => {
		if (errorsOf === null || openFailed === undefined) return;
		invoke<FailedDocument[]>("list_failed_documents", {
			folderId: errorsOf,
		}).then(setFailedDocs, () => setError("Could not read the error list."));
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
				setNotice("This is a large folder. Indexing it may take a while.");
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
			setNotice(`Queued ${d.relPath} again.`);
		});

	const savePatterns = () =>
		run(async () => {
			const s = await invoke<Settings>("set_exclude_patterns", { patterns });
			setPatterns(s.excludePatterns);
			setSavedPatterns(s.excludePatterns);
			setNotice("Exclude patterns saved. Folders are being rescanned.");
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
			<h2 id="folders-heading">Folders</h2>
			<p role="status">
				{progressText(status?.indexing ?? false, status?.progress ?? null)}
			</p>
			{folders.length === 0 ? (
				<p>No folders yet. Add one to start indexing.</p>
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
								{f.documents} documents, {f.failed} errors, {f.skipped} skipped
								{!f.enabled && " (paused, left out of search)"}
								{f.status === "unavailable" && " (unavailable)"}
							</span>{" "}
							{f.failed > 0 && (
								<button
									type="button"
									aria-expanded={errorsOf === f.id}
									onClick={() => setErrorsOf(errorsOf === f.id ? null : f.id)}
								>
									{errorsOf === f.id ? "Hide errors" : "Show errors"}
								</button>
							)}{" "}
							{confirming === f.id ? (
								<fieldset>
									<legend>
										Remove {f.label} from the index? Files on disk stay.
									</legend>
									<button
										type="button"
										disabled={busy}
										onClick={() => void remove(f)}
									>
										Remove
									</button>{" "}
									<button type="button" onClick={() => setConfirming(null)}>
										Cancel
									</button>
								</fieldset>
							) : (
								<button
									type="button"
									disabled={busy}
									onClick={() => setConfirming(f.id)}
									aria-label={`Remove ${f.label}`}
								>
									Remove…
								</button>
							)}
							{errorsOf === f.id && (
								<ul aria-label={`Errors in ${f.label}`}>
									{failedDocs.map((d) => (
										<li key={d.docId}>
											<span>{d.relPath}</span> <em>{REASONS[d.reason]}</em>{" "}
											<button
												type="button"
												disabled={busy || !f.enabled}
												onClick={() => void retry(d)}
												aria-label={`Retry ${d.relPath}`}
											>
												Retry
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
				Add folder…
			</button>
			<h3>
				<label htmlFor="exclude-patterns">Exclude patterns</label>
			</h3>
			<p id="exclude-help">
				One gitignore pattern per line, such as <code>drafts/</code> or{" "}
				<code>*.bak</code>. Applies to every folder.
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
				Save patterns
			</button>
			<h3 id="schedule-heading">Schedule</h3>
			<p role="status" aria-labelledby="schedule-heading">
				{scheduleText(schedule?.current ?? null)}
			</p>
			{sched && (
				<>
					<fieldset disabled={busy}>
						<legend>Indexing</legend>
						{(
							[
								["auto", "Automatic"],
								["paused", "Paused"],
								["run_now", "Index everything now"],
							] as const
						).map(([mode, label]) => (
							<label key={mode}>
								<input
									type="radio"
									name="schedule-mode"
									checked={sched.mode === mode}
									onChange={() => void saveSchedule({ ...sched, mode })}
								/>{" "}
								{label}
							</label>
						))}
					</fieldset>
					<fieldset disabled={busy}>
						<legend>Allowed hours</legend>
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
							Only index between
						</label>{" "}
						{sched.allowedHours && (
							<>
								<input
									type="time"
									aria-label="Start time"
									value={toTime(sched.allowedHours.startMinute)}
									onChange={(e) =>
										setHour("startMinute", e.currentTarget.value)
									}
								/>{" "}
								and{" "}
								<input
									type="time"
									aria-label="End time"
									value={toTime(sched.allowedHours.endMinute)}
									onChange={(e) => setHour("endMinute", e.currentTarget.value)}
								/>
							</>
						)}
					</fieldset>
					<label>
						On low battery, in Low Power Mode, or when your Mac is hot:{" "}
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
							<option value="pause">Pause indexing</option>
							<option value="one_worker">Keep going slowly</option>
						</select>
					</label>
				</>
			)}
			{notice && <p>{notice}</p>}
			{error && <p role="alert">{error}</p>}
		</section>
	);
}
