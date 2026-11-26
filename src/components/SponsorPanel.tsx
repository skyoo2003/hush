// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { APP_NAME, SPONSOR_URLS } from "../brand";
import { FUNDING_GOAL } from "../funding";
import { t } from "../i18n";
import { Group, Row } from "../settings/Form";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Switch } from "../ui/Switch";
import type { Settings } from "../ipc/Settings";
import type { SponsorLink } from "../ipc/SponsorLink";

export type Qr = Extract<SponsorLink, { kind: "qr" }>;


// Settings > Sponsor. The supporter flag is read here only, for the badge (scripts/check-supporter.sh).
export function SponsorPanel({ trayQr }: { trayQr: Qr | null }) {
	const [supporter, setSupporter] = useState<boolean | null>(null);
	const [qr, setQr] = useState<Qr | null>(null);
	const [notice, setNotice] = useState<string | null>(null);

	useEffect(() => {
		invoke<Settings>("get_settings").then(
			(s) => setSupporter(s.supporter),
			() => setNotice(t("security.readFailed")),
		);
	}, []);
	// The tray sends a QR when a channel is clicked with Air-Gap on; the window keeps it.
	useEffect(() => {
		if (trayQr) setQr(trayQr);
	}, [trayQr]);

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
		<>
			<Group
				title={t("main.sponsor")}
				description={
					<>
						<p>{t("sponsor.body", { app: APP_NAME })}</p>
						<p>{t("sponsor.goal", { goal: FUNDING_GOAL })}</p>
					</>
				}
			>
				<Row>
					{SPONSOR_URLS.map((url, i) => (
						<Button key={url} onClick={() => open(i)}>
							{t(`sponsor.channel.${i as 0 | 1}`)}
						</Button>
					))}
				</Row>
				{qr && (
					<figure className="form-qr">
						<img
							src={`data:image/svg+xml;utf8,${encodeURIComponent(qr.svg)}`}
							alt={t("sponsor.qrAlt", { url: qr.url })}
							width={200}
							height={200}
						/>
						<figcaption>
							{t("sponsor.qrHint")}{" "}
							<span className="form-mono" data-selectable>
								{qr.url}
							</span>
						</figcaption>
					</figure>
				)}
				{notice && (
					<Row>
						<span role="status">{notice}</span>
					</Row>
				)}
			</Group>
			<Group title={t("sponsor.iSponsored")}>
				<Row
					note={
						supporter ? (
							<p role="status">
								<Badge tone="accent">{t("sponsor.badge", { app: APP_NAME })}</Badge>{" "}
								{t("sponsor.badgeThemes")}
							</p>
						) : (
							t("sponsor.selfReported")
						)
					}
				>
					<Switch
						label={t("sponsor.iSponsored")}
						checked={supporter ?? false}
						disabled={supporter === null}
						onCheckedChange={toggle}
					/>
				</Row>
			</Group>
		</>
	);
}
