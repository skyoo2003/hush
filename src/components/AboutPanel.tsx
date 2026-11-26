// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t } from "../i18n";
import { Group, Row } from "../settings/Form";
import type { About } from "../ipc/About";
import type { LegalText } from "../ipc/LegalText";

// GPL-3.0 section 5(d) notices: copyright, no warranty, the license and its full text, and the
// source of this exact build. URLs are text, not links: the webview never requests a URL (rule 2).
function LegalDetails({ which, label }: { which: LegalText; label: string }) {
	const [text, setText] = useState<string | null>(null);
	return (
		<details
			onToggle={(e) => {
				if (e.currentTarget.open && text === null) {
					invoke<string>("read_legal_text", { which }).then(setText, () =>
						setText(t("about.readFailed")),
					);
				}
			}}
		>
			<summary>{label}</summary>
			<pre className="legal-text" data-selectable>{text}</pre>
		</details>
	);
}

export function AboutPanel() {
	const [about, setAbout] = useState<About | null>(null);

	useEffect(() => {
		invoke<About>("get_about").then(setAbout, () => {});
	}, []);

	if (!about) return null;
	return (
		<Group title={t("about.heading", { app: about.name })}>
			<Row
				label={
					<>
						<strong>{about.name}</strong>
						<div className="form-row-note">
							{t("about.version", { version: about.version })}
						</div>
					</>
				}
			/>
			<Row note={<p>{t("about.warranty")}</p>}>{about.copyright}</Row>
			<Row
				label={t("about.license", { license: about.license })}
				note={<span className="form-mono" data-selectable>{about.licenseUrl}</span>}
			/>
			<Row
				label={t("about.source")}
				note={<span className="form-mono" data-selectable>{about.sourceUrl}</span>}
			/>
			<Row>
				<LegalDetails which="license" label={t("about.showLicense")} />
			</Row>
			<Row>
				<LegalDetails which="third_party_notices" label={t("about.showNotices")} />
			</Row>
		</Group>
	);
}
