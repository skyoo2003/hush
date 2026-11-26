// SPDX-License-Identifier: GPL-3.0-or-later
import { Sparkles } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Trust.css";

type Props = { label: string } & Omit<
	ComponentPropsWithRef<"span">,
	"children"
>;

export function AiLabel({ label, className, ...rest }: Props) {
	return (
		<span {...rest} className={`trust-inline ai-label ${className ?? ""}`}>
			<Sparkles size={12} strokeWidth={1.5} aria-hidden />
			{label}
		</span>
	);
}
