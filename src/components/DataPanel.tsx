// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";

// Export diagnostics (saved locally, never sent) and Delete all indexes (two steps: the list
// of what goes, then the confirm button).
export function DataPanel() {
	const [status, setStatus] = useState<string | null>(null);
	const [confirming, setConfirming] = useState(false);
	const [deleting, setDeleting] = useState(false);

	const exportDiagnostics = () => {
		setStatus(null);
		invoke<boolean>("export_diagnostics").then(
			(saved) => setStatus(saved ? t("diag.saved") : null),
			() => setStatus(t("diag.failed")),
		);
	};

	const deleteAll = () => {
		setDeleting(true);
		setStatus(null);
		// On success the app restarts, so only a failure comes back.
		invoke("delete_all_indexes").catch(() => {
			setStatus(t("delete.failed"));
			setDeleting(false);
		});
	};

	return (
		<section aria-labelledby="data-heading">
			<h2 id="data-heading">{t("data.heading")}</h2>
			<p>{t("diag.body")}</p>
			<button type="button" onClick={exportDiagnostics}>
				{t("diag.export")}
			</button>
			{confirming ? (
				<div role="alertdialog" aria-labelledby="delete-title">
					<p id="delete-title">{t("delete.title")}</p>
					<ul>
						<li>{t("delete.item.db")}</li>
						<li>{t("delete.item.folders")}</li>
						<li>{t("delete.item.settings")}</li>
						<li>{t("delete.item.key")}</li>
					</ul>
					<p>{t("delete.kept")}</p>
					<p>{t("delete.restart", { app: APP_NAME })}</p>
					<button type="button" disabled={deleting} onClick={deleteAll}>
						{t("delete.confirm")}
					</button>
					<button
						type="button"
						disabled={deleting}
						onClick={() => setConfirming(false)}
					>
						{t("action.cancel")}
					</button>
				</div>
			) : (
				<button type="button" onClick={() => setConfirming(true)}>
					{t("delete.button")}
				</button>
			)}
			{status && <p role="status">{status}</p>}
		</section>
	);
}
