// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { t } from "../i18n";
import type { DownloadProgress } from "../ipc/DownloadProgress";
import type { ImportedFile } from "../ipc/ImportedFile";
import type { ModelInfo } from "../ipc/ModelInfo";
import type { Settings } from "../ipc/Settings";
import {
	formatBytes,
	importText,
	MODEL_PROGRESS_EVENT,
	missingEmbedding,
	modelErrorText,
	percent,
} from "../models";

function FileList({ model }: { model: ModelInfo }) {
	return (
		<ul>
			{model.files.map((f) => (
				<li key={f.name}>
					<code>{f.name}</code> ({formatBytes(f.size)})
					{f.installed ? t("models.file.installed") : ""}
					<br />
					{/* Text, not a link: the webview must never request a URL itself (rule 2). */}
					<code>{f.url}</code>
					<br />
					SHA-256 <code>{f.sha256}</code>
				</li>
			))}
		</ul>
	);
}

export function SecurityPanel() {
	// null until get_settings answers; the switch stays disabled until then.
	const [airGap, setAirGap] = useState<boolean | null>(null);
	const [toggling, setToggling] = useState(false);
	const [models, setModels] = useState<ModelInfo[]>([]);
	const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
	const [notice, setNotice] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	const refresh = useCallback(() => {
		invoke<Settings>("get_settings").then(
			(s) => setAirGap(s.airGap),
			() => setError(t("security.readFailed")),
		);
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

	const toggle = async (next: boolean) => {
		setToggling(true);
		setError(null);
		try {
			await invoke("set_air_gap", { enabled: next });
			setAirGap(next);
		} catch {
			setError(t("security.saveFailed"));
		} finally {
			setToggling(false);
		}
	};

	// airGapOffForDownload is true only from the first-run button the user clicked.
	const download = async (id: string, airGapOffForDownload: boolean) => {
		setError(null);
		setNotice(null);
		setModels((ms) => ms.map((m) => (m.id === id ? { ...m, downloading: true } : m)));
		if (airGapOffForDownload) setAirGap(false);
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
			const r = await invoke<ImportedFile | null>("import_model_file");
			if (r) setNotice(importText(r, models));
		} catch (e) {
			setError(modelErrorText(e));
		} finally {
			refresh();
		}
	};

	const firstRun = missingEmbedding(models);

	return (
		<section aria-labelledby="security-heading">
			<h2 id="security-heading">{t("security.heading")}</h2>
			<label>
				<input
					type="checkbox"
					role="switch"
					aria-checked={airGap ?? true}
					checked={airGap ?? true}
					disabled={airGap === null || toggling}
					onChange={(e) => void toggle(e.target.checked)}
				/>
				{t("security.airGapLabel")}
			</label>
			<p>
				{airGap === false ? t("security.airGapOff") : t("security.airGapOn")}
			</p>

			<h3>{t("models.heading")}</h3>
			{firstRun && (
				<fieldset>
					<legend>{t("models.firstRun.legend")}</legend>
					<p>
						{t("models.firstRun.install", {
							id: firstRun.id,
							size: formatBytes(firstRun.size),
						})}
					</p>
					<button
						type="button"
						disabled={firstRun.downloading || airGap === null}
						onClick={() => void download(firstRun.id, airGap === true)}
					>
						{airGap
							? t("models.firstRun.downloadAirGap")
							: t("models.download")}
					</button>
					{airGap && <p>{t("models.firstRun.airGapNote")}</p>}
					<button type="button" onClick={() => void importFile()}>
						{t("models.importFile")}
					</button>
					<p>{t("models.firstRun.offline")}</p>
					<FileList model={firstRun} />
				</fieldset>
			)}

			<ul>
				{models.map((m) => {
					const p = progress[m.id];
					return (
						<li key={m.id}>
							<strong>{m.id}</strong> · {t(`models.kind.${m.kind}`)} ·{" "}
							{m.license} · {formatBytes(m.size)}
							{m.installed ? (
								<span> · {t("models.installedBadge")}</span>
							) : m.downloading ? (
								<span>
									{" "}
									<progress
										max={100}
										value={p ? percent(p.done, p.total) : undefined}
										aria-label={t("models.downloading", { id: m.id })}
									/>{" "}
									{p ? `${percent(p.done, p.total)}%` : t("models.starting")}{" "}
									<button
										type="button"
										onClick={() => void invoke("cancel_model_download", { id: m.id })}
									>
										{t("action.cancel")}
									</button>
								</span>
							) : (
								<span>
									{" "}
									<button
										type="button"
										disabled={airGap !== false}
										onClick={() => void download(m.id, false)}
									>
										{t("models.download")}
									</button>
									{airGap !== false && ` ${t("models.airGapOnNote")}`}
								</span>
							)}
							{!m.installed && (
								<details>
									<summary>{t("models.manualFiles")}</summary>
									<FileList model={m} />
								</details>
							)}
						</li>
					);
				})}
			</ul>
			<button type="button" onClick={() => void importFile()}>
				{t("models.importModelFile")}
			</button>
			{notice && <p role="status">{notice}</p>}
			{error && <p role="alert">{error}</p>}
		</section>
	);
}
