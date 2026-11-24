// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import type { Settings } from "../ipc/Settings";
import { SPONSOR_HEADING_ID } from "./SponsorPanel";

// One-time milestone banner at the bottom of the main window. Rust decides when it shows;
// dismissing it (or turning on "I sponsored") hides it for good.
export function SponsorBanner() {
	const [show, setShow] = useState(false);

	useEffect(() => {
		invoke<Settings>("get_settings").then(
			(s) => setShow(s.sponsorBanner),
			() => {},
		);
	}, []);

	if (!show) return null;
	return (
		<aside aria-label={t("main.sponsor")} className="sponsor-banner">
			<p>{t("banner.text", { app: APP_NAME })}</p>
			<button
				type="button"
				onClick={() =>
					document.getElementById(SPONSOR_HEADING_ID)?.scrollIntoView()
				}
			>
				{t("banner.open")}
			</button>
			<button
				type="button"
				onClick={() =>
					invoke<Settings>("dismiss_sponsor_banner").then(
						(s) => setShow(s.sponsorBanner),
						() => setShow(false),
					)
				}
			>
				{t("banner.dismiss")}
			</button>
		</aside>
	);
}
