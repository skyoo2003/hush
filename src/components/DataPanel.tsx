// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import { Group, Row } from "../settings/Form";
import { Alert } from "../ui/Alert";
import { Button } from "../ui/Button";

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
		<Group title={t("data.heading")}>
			<Row label={t("diag.body")}>
				<Button onClick={exportDiagnostics}>{t("diag.export")}</Button>
			</Row>
			<Row
				label={t("delete.title")}
				note={
					<>
						<ul className="form-list">
							<li>{t("delete.item.db")}</li>
							<li>{t("delete.item.folders")}</li>
							<li>{t("delete.item.settings")}</li>
							<li>{t("delete.item.key")}</li>
						</ul>
						<p>{t("delete.kept")}</p>
					</>
				}
			>
				<Button
					variant="destructive"
					disabled={deleting}
					onClick={() => setConfirming(true)}
				>
					{t("delete.button")}
				</Button>
			</Row>
			{status && (
				<Row>
					<span role="status">{status}</span>
				</Row>
			)}
			<Alert
				open={confirming}
				onOpenChange={setConfirming}
				tone="destructive"
				title={t("delete.confirm")}
				message={t("delete.restart", { app: APP_NAME })}
				confirmLabel={t("delete.confirm")}
				cancelLabel={t("action.cancel")}
				onConfirm={deleteAll}
			/>
		</Group>
	);
}
