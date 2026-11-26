// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import type { Settings } from "../ipc/Settings";
import { Banner } from "../ui/Banner";
import { Button } from "../ui/Button";

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
	const dismiss = () =>
		invoke<Settings>("dismiss_sponsor_banner").then(
			(s) => setShow(s.sponsorBanner),
			() => setShow(false),
		);
	return (
		<Banner
			tone="info"
			className="sponsor-banner"
			aria-label={t("main.sponsor")}
			onDismiss={() => void dismiss()}
			dismissLabel={t("banner.dismiss")}
			action={
				<Button
					size="small"
					onClick={() => void invoke("open_settings", { pane: "sponsor" })}
				>
					{t("banner.open")}
				</Button>
			}
		>
			{t("banner.text", { app: APP_NAME })}
		</Banner>
	);
}
