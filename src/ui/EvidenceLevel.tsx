// SPDX-License-Identifier: GPL-3.0-or-later
import { CircleSlash, SignalHigh, SignalLow } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Trust.css";

const ICONS = {
	strong: SignalHigh,
	weak: SignalLow,
	none: CircleSlash,
} as const;

type Props = { level: keyof typeof ICONS; label: string } & Omit<
	ComponentPropsWithRef<"span">,
	"children"
>;

export function EvidenceLevel({ level, label, className, ...rest }: Props) {
	const Icon = ICONS[level];
	return (
		<span
			{...rest}
			className={`trust-inline evidence evidence-${level} ${className ?? ""}`}
		>
			<Icon size={14} strokeWidth={1.5} aria-hidden />
			{label}
		</span>
	);
}
