// SPDX-License-Identifier: GPL-3.0-or-later
import { Columns2, History, MessageSquareText } from "lucide-react";
import { APP_NAME } from "../brand";
import { t } from "../i18n";
import { EmptyState } from "../ui/EmptyState";

const PAGES = {
	ask: { icon: MessageSquareText, text: "planned.ask" },
	compare: { icon: Columns2, text: "planned.compare" },
	changes: { icon: History, text: "planned.changes" },
} as const;

/** Ask, Compare, and Changes have no backend yet (roadmap weeks 9–11). A neutral note, no prompt. */
export function PlannedPage({ kind }: { kind: keyof typeof PAGES }) {
	const p = PAGES[kind];
	return (
		<EmptyState
			icon={p.icon}
			title={t("planned.title")}
			description={t(p.text, { app: APP_NAME })}
		/>
	);
}
