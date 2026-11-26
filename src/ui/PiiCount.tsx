// SPDX-License-Identifier: GPL-3.0-or-later
import { EyeOff } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Trust.css";

type Props = { count: number; label: string } & Omit<
	ComponentPropsWithRef<"span">,
	"children"
>;

/** `label` is the full localized string; its count digits render in mono. */
export function PiiCount({ count, label, className, ...rest }: Props) {
	const n = String(count);
	const at = label.indexOf(n);
	return (
		<span
			{...rest}
			className={`trust-inline pii-count ${className ?? ""}`}
			data-masked={count > 0}
		>
			<EyeOff size={14} strokeWidth={1.5} aria-hidden />
			{at < 0 ? (
				label
			) : (
				<span>
					{label.slice(0, at)}
					<span className="trust-mono">{n}</span>
					{label.slice(at + n.length)}
				</span>
			)}
		</span>
	);
}
