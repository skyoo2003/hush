// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { FolderX } from "lucide-react";
import { useEffect, useState } from "react";
import { t } from "../i18n";
import type { FailedDocument } from "../ipc/FailedDocument";
import type { FolderInfo } from "../ipc/FolderInfo";
import { Banner } from "../ui/Banner";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Text } from "../ui/Text";

type Props = {
	folder: FolderInfo | undefined;
	onSearchIn(folder: FolderInfo): void;
	onChanged(): void;
};

export const folderStatusText = (f: FolderInfo) =>
	f.status === "unavailable"
		? t("folder.statusUnavailable")
		: f.enabled
			? t("folder.statusActive")
			: t("folder.statusPaused");

/** One watched folder: its state, counts, and the files that failed, with Retry. */
export function FolderPage({ folder, onSearchIn, onChanged }: Props) {
	const [failed, setFailed] = useState<FailedDocument[]>([]);
	const [notice, setNotice] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const id = folder?.id;
	const failedCount = folder?.failed;

	// Reloads when the count changes, e.g. after a retry succeeds.
	useEffect(() => {
		setNotice(null);
		setError(null);
		if (id === undefined || failedCount === undefined) return;
		invoke<FailedDocument[]>("list_failed_documents", { folderId: id }).then(
			setFailed,
			() => setError(t("folders.errorListFailed")),
		);
	}, [id, failedCount]);

	if (!folder)
		return <EmptyState icon={FolderX} title={t("folder.gone")} />;

	const run = async (action: () => Promise<void>) => {
		setBusy(true);
		setError(null);
		setNotice(null);
		try {
			await action();
		} catch {
			setError(t("error.generic"));
		} finally {
			setBusy(false);
			onChanged();
		}
	};

	return (
		<div className="folder-page">
			<header className="folder-page-header">
				<Text as="h1" variant="title2">
					{folder.label}
				</Text>
				<code className="folder-page-path" data-selectable>
					{folder.path}
				</code>
				<Text tone="secondary">
					{folderStatusText(folder)} ·{" "}
					{t("folders.counts", {
						documents: folder.documents,
						errors: folder.failed,
						skipped: folder.skipped,
					})}
				</Text>
				<div className="folder-page-actions">
					<Button
						variant="primary"
						disabled={!folder.enabled}
						onClick={() => onSearchIn(folder)}
					>
						{t("folder.searchIn")}
					</Button>
					<Button
						disabled={busy}
						onClick={() =>
							void run(() =>
								invoke("set_folder_enabled", {
									id: folder.id,
									enabled: !folder.enabled,
								}),
							)
						}
					>
						{folder.enabled ? t("folder.pause") : t("folder.resume")}
					</Button>
					<Button
						variant="plain"
						onClick={() => void invoke("open_settings", { pane: "folders" })}
					>
						{t("folder.showSettings")}
					</Button>
				</div>
			</header>
			{error && <Banner tone="error">{error}</Banner>}
			{notice && <Banner tone="success">{notice}</Banner>}
			<Text as="h2" variant="headline">
				{t("folder.failedHeading")}
			</Text>
			{folder.failed === 0 ? (
				<Text tone="secondary">{t("folder.noFailed")}</Text>
			) : (
				<ul
					className="folder-failed"
					aria-label={t("folders.errorsIn", { label: folder.label })}
				>
					{failed.map((d) => (
						<li key={d.docId}>
							<code data-selectable>{d.relPath}</code>
							<Text tone="secondary">{t(`folders.reason.${d.reason}`)}</Text>
							<Button
								size="small"
								disabled={busy || !folder.enabled}
								aria-label={t("folders.retryLabel", { path: d.relPath })}
								onClick={() =>
									void run(async () => {
										await invoke("retry_document", { docId: d.docId });
										setNotice(t("folders.retryQueued", { path: d.relPath }));
									})
								}
							>
								{t("folders.retry")}
							</Button>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}
