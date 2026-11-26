// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Toolbar.css";

export type ToolbarProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
	label: string;
	/** Leaves room for the traffic lights (titleBarStyle Overlay). */
	inset?: boolean;
	leading?: ReactNode;
	center?: ReactNode;
	trailing?: ReactNode;
};

// data-tauri-drag-region drags only from the element that carries it, so the empty slot areas
// carry it too while the controls inside stay clickable.
export function Toolbar({
	label,
	inset = true,
	leading,
	center,
	trailing,
	className,
	...rest
}: ToolbarProps) {
	return (
		<div
			{...rest}
			role="toolbar"
			aria-label={label}
			data-tauri-drag-region
			data-inset={inset || undefined}
			className={`toolbar${className ? ` ${className}` : ""}`}
		>
			<div className="toolbar-leading" data-tauri-drag-region>
				{leading}
			</div>
			<div className="toolbar-center" data-tauri-drag-region>
				{center}
			</div>
			<div className="toolbar-trailing" data-tauri-drag-region>
				{trailing}
			</div>
		</div>
	);
}
