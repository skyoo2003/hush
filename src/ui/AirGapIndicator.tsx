// SPDX-License-Identifier: GPL-3.0-or-later
import { ShieldCheck, ShieldOff } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Trust.css";

type Props = {
	on: boolean;
	label: string;
	onClick(): void;
} & Omit<ComponentPropsWithRef<"button">, "children" | "onClick">;

/** Toolbar Air-Gap status; the click opens Settings > Privacy. */
export function AirGapIndicator({ on, label, className, ...rest }: Props) {
	const Icon = on ? ShieldCheck : ShieldOff;
	return (
		<button
			type="button"
			{...rest}
			className={`airgap ${className ?? ""}`}
			data-on={on}
		>
			<Icon size={16} strokeWidth={1.5} aria-hidden />
			{label}
		</button>
	);
}
