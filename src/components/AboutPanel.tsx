// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t } from "../i18n";
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
		<section aria-labelledby="about-heading">
			<h2 id="about-heading">{t("about.heading", { app: about.name })}</h2>
			<p>{t("about.version", { version: about.version })}</p>
			<p>{about.copyright}</p>
			<p>{t("about.warranty")}</p>
			<p>
				{t("about.license", { license: about.license })}{" "}
				<code>{about.licenseUrl}</code>
			</p>
			<p>
				{t("about.source")} <code>{about.sourceUrl}</code>
			</p>
			<LegalDetails which="license" label={t("about.showLicense")} />
			<LegalDetails
				which="third_party_notices"
				label={t("about.showNotices")}
			/>
		</section>
	);
}
