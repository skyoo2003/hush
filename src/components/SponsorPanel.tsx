// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { APP_NAME, SPONSOR_URLS } from "../brand";
import { FUNDING_GOAL } from "../funding";
import { t } from "../i18n";
import type { Settings } from "../ipc/Settings";
import type { SponsorLink } from "../ipc/SponsorLink";

type Qr = Extract<SponsorLink, { kind: "qr" }>;

export const SPONSOR_HEADING_ID = "sponsor-heading";
const QR_EVENT = "sponsor://qr";

// Settings > Sponsor. The supporter flag is read here only, for the badge (scripts/check-supporter.sh).
export function SponsorPanel() {
	const [supporter, setSupporter] = useState<boolean | null>(null);
	const [qr, setQr] = useState<Qr | null>(null);
	const [notice, setNotice] = useState<string | null>(null);

	useEffect(() => {
		invoke<Settings>("get_settings").then(
			(s) => setSupporter(s.supporter),
			() => setNotice(t("security.readFailed")),
		);
		// The tray sends a QR here when it is clicked with Air-Gap on.
		const unlisten = listen<SponsorLink>(QR_EVENT, (e) => {
			if (e.payload.kind === "qr") setQr(e.payload);
		});
		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	function open(channel: number) {
		setNotice(null);
		// Rust decides: the external browser with Air-Gap off, a local QR code with it on.
		invoke<SponsorLink>("open_sponsor", { channel }).then(
			(link) => {
				setQr(link.kind === "qr" ? link : null);
				if (link.kind === "browser") setNotice(t("sponsor.opened"));
			},
			() => setNotice(t("sponsor.failed")),
		);
	}

	function toggle(on: boolean) {
		invoke<Settings>("set_supporter", { on }).then(
			(s) => setSupporter(s.supporter),
			() => setNotice(t("sponsor.failed")),
		);
	}

	return (
		<section aria-labelledby={SPONSOR_HEADING_ID}>
			<h2 id={SPONSOR_HEADING_ID}>{t("main.sponsor")}</h2>
			<p>{t("sponsor.body", { app: APP_NAME })}</p>
			<p>{t("sponsor.goal", { goal: FUNDING_GOAL })}</p>
			{SPONSOR_URLS.map((url, i) => (
				<button key={url} type="button" onClick={() => open(i)}>
					{t(`sponsor.channel.${i as 0 | 1}`)}
				</button>
			))}
			{qr && (
				<figure>
					<img
						src={`data:image/svg+xml;utf8,${encodeURIComponent(qr.svg)}`}
						alt={t("sponsor.qrAlt", { url: qr.url })}
						width={200}
						height={200}
					/>
					<figcaption>
						{t("sponsor.qrHint")} <code>{qr.url}</code>
					</figcaption>
				</figure>
			)}
			{notice && <p role="status">{notice}</p>}
			<label>
				<input
					type="checkbox"
					checked={supporter ?? false}
					disabled={supporter === null}
					onChange={(e) => toggle(e.target.checked)}
				/>
				{t("sponsor.iSponsored")}
			</label>
			<p>{t("sponsor.selfReported")}</p>
			{supporter && (
				<p role="status">
					<strong>{t("sponsor.badge", { app: APP_NAME })}</strong>{" "}
					{t("sponsor.badgeThemes")}
				</p>
			)}
		</section>
	);
}
