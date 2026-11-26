// SPDX-License-Identifier: GPL-3.0-or-later
// Settings › Privacy & Air-Gap, Models, and General › Software Update. All three read the same
// Air-Gap state, which the settings window owns (`AirGap`), so a toggle in one pane shows in all.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Download, FileInput } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { t } from "../i18n";
import type { AvailableUpdate } from "../ipc/AvailableUpdate";
import type { DownloadProgress } from "../ipc/DownloadProgress";
import type { ImportedFile } from "../ipc/ImportedFile";
import type { ModelInfo } from "../ipc/ModelInfo";
import {
	formatBytes,
	importText,
	MODEL_PROGRESS_EVENT,
	missingEmbedding,
	modelErrorText,
	percent,
} from "../models";
import { Group, Row } from "../settings/Form";
import { Badge } from "../ui/Badge";
import { Banner } from "../ui/Banner";
import { Button } from "../ui/Button";
import { ProgressBar } from "../ui/ProgressBar";
import { Switch } from "../ui/Switch";
import { updateErrorText } from "../update";

/** null until get_settings answers; controls that depend on it stay disabled until then. */
export type AirGap = {
	on: boolean | null;
	set(on: boolean): void;
	error: string | null;
};

function FileList({ model }: { model: ModelInfo }) {
	return (
		<ul className="form-list">
			{model.files.map((f) => (
				<li key={f.name} className="form-mono" data-selectable>
					{f.name} ({formatBytes(f.size)})
					{f.installed ? t("models.file.installed") : ""}
					<br />
					{/* Text, not a link: the webview must never request a URL itself (rule 2). */}
					{f.url}
					<br />
					SHA-256 {f.sha256}
				</li>
			))}
		</ul>
	);
}

export function AirGapPanel({ airGap }: { airGap: AirGap }) {
	const [toggling, setToggling] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const toggle = async (next: boolean) => {
		setToggling(true);
		setError(null);
		try {
			await invoke("set_air_gap", { enabled: next });
			airGap.set(next);
		} catch {
			setError(t("security.saveFailed"));
		} finally {
			setToggling(false);
		}
	};

	return (
		<Group title={t("settings.privacy")}>
			<Row
				note={
					<>
						<p>
							{airGap.on === false
								? t("security.airGapOff")
								: t("security.airGapOn")}
						</p>
						<p>{t("privacy.verify")}</p>
					</>
				}
			>
				<Switch
					label={t("security.airGapLabel")}
					checked={airGap.on ?? true}
					disabled={airGap.on === null || toggling}
					onCheckedChange={(v) => void toggle(v)}
				/>
			</Row>
			{(error ?? airGap.error) && (
				<Banner tone="error">{error ?? airGap.error}</Banner>
			)}
		</Group>
	);
}

export function ModelsPanel({ airGap }: { airGap: AirGap }) {
	const [models, setModels] = useState<ModelInfo[]>([]);
	const [progress, setProgress] = useState<Record<string, DownloadProgress>>(
		{},
	);
	const [notice, setNotice] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	const refresh = useCallback(() => {
		invoke<ModelInfo[]>("list_models").then(setModels, () =>
			setError(t("models.listFailed")),
		);
	}, []);

	useEffect(() => {
		refresh();
		const unlisten = listen<DownloadProgress>(MODEL_PROGRESS_EVENT, (e) =>
			setProgress((p) => ({ ...p, [e.payload.id]: e.payload })),
		);
		return () => {
			void unlisten.then((f) => f());
		};
	}, [refresh]);

	// airGapOffForDownload is true only from the first-run button the user clicked.
	const download = async (id: string, airGapOffForDownload: boolean) => {
		setError(null);
		setNotice(null);
		setModels((ms) =>
			ms.map((m) => (m.id === id ? { ...m, downloading: true } : m)),
		);
		if (airGapOffForDownload) airGap.set(false);
		try {
			await invoke("download_model", { id, airGapOffForDownload });
			setNotice(t("models.installed", { id }));
		} catch (e) {
			setError(modelErrorText(e));
		} finally {
			setProgress(({ [id]: _, ...rest }) => rest);
			refresh();
		}
	};

	const importFile = async () => {
		setError(null);
		setNotice(null);
		try {
			const r = await invoke<ImportedFile[] | null>("import_model_file");
			if (r) setNotice(importText(r, models));
		} catch (e) {
			setError(modelErrorText(e));
		} finally {
			refresh();
		}
	};

	const firstRun = missingEmbedding(models);

	return (
		<>
			{firstRun && (
				<Group title={t("models.firstRun.legend")}>
					<Row
						label={t("models.firstRun.install", {
							id: firstRun.id,
							size: formatBytes(firstRun.size),
						})}
						note={airGap.on ? t("models.firstRun.airGapNote") : undefined}
					>
						<Button
							variant="primary"
							icon={Download}
							disabled={firstRun.downloading || airGap.on === null}
							onClick={() => void download(firstRun.id, airGap.on === true)}
						>
							{airGap.on
								? t("models.firstRun.downloadAirGap")
								: t("models.download")}
						</Button>
					</Row>
					<Row
						label={t("models.firstRun.offline")}
						note={
							<details>
								<summary>{t("models.manualFiles")}</summary>
								<FileList model={firstRun} />
							</details>
						}
					>
						<Button icon={FileInput} onClick={() => void importFile()}>
							{t("models.importFile")}
						</Button>
					</Row>
				</Group>
			)}
			<Group title={t("settings.models")}>
				{models.map((m) => {
					const p = progress[m.id];
					return (
						<Row
							key={m.id}
							label={
								<>
									<strong>{m.id}</strong>
									<div className="form-row-note">
										{t(`models.kind.${m.kind}`)} · {m.license} ·{" "}
										{formatBytes(m.size)}
									</div>
								</>
							}
							note={
								<>
									{!m.installed &&
										!m.downloading &&
										airGap.on !== false &&
										t("models.airGapOnNote")}
									{!m.installed && (
										<details>
											<summary>{t("models.manualFiles")}</summary>
											<FileList model={m} />
										</details>
									)}
								</>
							}
						>
							{m.installed ? (
								<Badge tone="success">{t("models.installedBadge")}</Badge>
							) : m.downloading ? (
								<>
									<ProgressBar
										label={t("models.downloading", { id: m.id })}
										value={p ? percent(p.done, p.total) / 100 : undefined}
									/>
									<span>
										{p ? `${percent(p.done, p.total)}%` : t("models.starting")}
									</span>
									<Button
										size="small"
										onClick={() =>
											void invoke("cancel_model_download", { id: m.id })
										}
									>
										{t("action.cancel")}
									</Button>
								</>
							) : (
								<Button
									size="small"
									icon={Download}
									disabled={airGap.on !== false}
									onClick={() => void download(m.id, false)}
								>
									{t("models.download")}
								</Button>
							)}
						</Row>
					);
				})}
				<Row>
					<Button icon={FileInput} onClick={() => void importFile()}>
						{t("models.importModelFile")}
					</Button>
				</Row>
			</Group>
			{notice && <Banner tone="success">{notice}</Banner>}
			{error && <Banner tone="error">{error}</Banner>}
		</>
	);
}

export function UpdatePanel({ airGap }: { airGap: AirGap }) {
	const [updating, setUpdating] = useState(false);
	const [status, setStatus] = useState<string | null>(null);
	const [available, setAvailable] = useState<string | null>(null);

	const check = async () => {
		setUpdating(true);
		setStatus(t("update.checking"));
		setAvailable(null);
		try {
			const u = await invoke<AvailableUpdate | null>("check_for_update");
			setAvailable(u?.version ?? null);
			setStatus(
				u
					? t("update.available", { version: u.version })
					: t("update.upToDate"),
			);
		} catch (e) {
			setStatus(updateErrorText(e));
		} finally {
			setUpdating(false);
		}
	};

	// On success the app relaunches, so only a failure comes back.
	const install = async () => {
		setUpdating(true);
		setStatus(t("update.installing"));
		try {
			await invoke("install_update");
		} catch (e) {
			setAvailable(null);
			setStatus(updateErrorText(e));
		} finally {
			setUpdating(false);
		}
	};

	const off = airGap.on !== false;
	return (
		<Group title={t("update.heading")} description={t("update.manualNote")}>
			<Row
				label={<span role="status">{status}</span>}
				note={off ? t("update.airGapOnNote") : undefined}
			>
				<Button disabled={off || updating} onClick={() => void check()}>
					{t("update.check")}
				</Button>
				{available && (
					<Button
						variant="primary"
						disabled={off || updating}
						onClick={() => void install()}
					>
						{t("update.install")}
					</Button>
				)}
			</Row>
		</Group>
	);
}
