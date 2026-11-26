// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef, ReactNode } from "react";
import { Tooltip } from "./Tooltip";
import "./Trust.css";

type Props = {
	index: number;
	label: string;
	location?: string;
	excerpt?: ReactNode;
	onOpen(): void;
} & Omit<ComponentPropsWithRef<"button">, "children" | "onClick">;

/** `[n] label · location`; click jumps to the source, hover shows the excerpt. */
export function SourceChip({
	index,
	label,
	location,
	excerpt,
	onOpen,
	className,
	...rest
}: Props) {
	const chip = (
		<button
			type="button"
			// The tooltip shows the full label when there is one; otherwise the native title does.
			title={excerpt ? undefined : label}
			{...rest}
			className={`source-chip ${className ?? ""}`}
			onClick={onOpen}
		>
			<span className="source-chip-index">[{index}]</span>
			<span className="source-chip-label">{label}</span>
			{location && <span className="source-chip-location">· {location}</span>}
		</button>
	);
	if (!excerpt) return chip;
	return (
		<Tooltip
			content={
				<>
					<div className="source-chip-tip-label">{label}</div>
					<div data-selectable>{excerpt}</div>
				</>
			}
		>
			{chip}
		</Tooltip>
	);
}
