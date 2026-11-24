// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { DownloadProgress } from "../ipc/DownloadProgress";
import type { ImportedFile } from "../ipc/ImportedFile";
import type { ModelInfo } from "../ipc/ModelInfo";
import type { ModelKind } from "../ipc/ModelKind";
import type { Settings } from "../ipc/Settings";
import {
	formatBytes,
	importText,
	MODEL_PROGRESS_EVENT,
	missingEmbedding,
	modelErrorText,
	percent,
} from "../models";

const KINDS: Record<ModelKind, string> = {
	embedding: "Embedding",
	reranker: "Reranker",
	llm: "Language model",
};

function FileList({ model }: { model: ModelInfo }) {
	return (
		<ul>
			{model.files.map((f) => (
				<li key={f.name}>
					<code>{f.name}</code> ({formatBytes(f.size)}){f.installed ? ", installed" : ""}
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
			() => setError("Could not read the Air-Gap setting."),
		);
		invoke<ModelInfo[]>("list_models").then(setModels, () =>
			setError("Could not read the model list."),
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
			setError("Could not save the Air-Gap setting. It is unchanged.");
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
			setNotice(`${id} is installed.`);
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
			<h2 id="security-heading">Security</h2>
			<label>
				<input
					type="checkbox"
					role="switch"
					aria-checked={airGap ?? true}
					checked={airGap ?? true}
					disabled={airGap === null || toggling}
					onChange={(e) => void toggle(e.target.checked)}
				/>
				Air-Gap mode
			</label>
			<p>
				{airGap === false
					? "Off. Only model downloads and update checks may reach the network."
					: "On. Nothing leaves this computer."}
			</p>

			<h3>Models</h3>
			{firstRun && (
				<fieldset>
					<legend>No embedding model is installed, so search and indexing are off.</legend>
					<p>
						Install {firstRun.id} ({formatBytes(firstRun.size)}) one of two ways.
					</p>
					<button
						type="button"
						disabled={firstRun.downloading || airGap === null}
						onClick={() => void download(firstRun.id, airGap === true)}
					>
						{airGap ? "Turn off Air-Gap for this download" : "Download"}
					</button>
					{airGap && (
						<p>
							Air-Gap turns back on when the download ends. Only huggingface.co and its download
							servers are contacted, and nothing about your documents is sent.
						</p>
					)}
					<button type="button" onClick={() => void importFile()}>
						Import a file…
					</button>
					<p>
						On a computer with no network, download these files on another computer, copy them
						over, and import each one:
					</p>
					<FileList model={firstRun} />
				</fieldset>
			)}

			<ul>
				{models.map((m) => {
					const p = progress[m.id];
					return (
						<li key={m.id}>
							<strong>{m.id}</strong> · {KINDS[m.kind]} · {m.license} · {formatBytes(m.size)}
							{m.installed ? (
								<span> · Installed</span>
							) : m.downloading ? (
								<span>
									{" "}
									<progress
										max={100}
										value={p ? percent(p.done, p.total) : undefined}
										aria-label={`Downloading ${m.id}`}
									/>{" "}
									{p ? `${percent(p.done, p.total)}%` : "Starting…"}{" "}
									<button
										type="button"
										onClick={() => void invoke("cancel_model_download", { id: m.id })}
									>
										Cancel
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
										Download
									</button>
									{airGap !== false && " Air-Gap is on, so downloads are off. Import the files instead."}
								</span>
							)}
							{!m.installed && (
								<details>
									<summary>Files for a manual import</summary>
									<FileList model={m} />
								</details>
							)}
						</li>
					);
				})}
			</ul>
			<button type="button" onClick={() => void importFile()}>
				Import a model file…
			</button>
			{notice && <p role="status">{notice}</p>}
			{error && <p role="alert">{error}</p>}
		</section>
	);
}
