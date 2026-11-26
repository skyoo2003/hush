// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef } from "react";
import "./ProgressBar.css";

export type ProgressBarProps = Omit<
	ComponentPropsWithRef<"div">,
	"children"
> & {
	label: string;
	/** 0–1; undefined is indeterminate. */
	value?: number;
};

export function ProgressBar({
	label,
	value,
	className,
	...rest
}: ProgressBarProps) {
	const percent =
		value === undefined
			? undefined
			: Math.round(Math.min(1, Math.max(0, value)) * 100);
	return (
		<div
			{...rest}
			role="progressbar"
			aria-label={label}
			aria-valuemin={0}
			aria-valuemax={100}
			aria-valuenow={percent}
			className={`progress-bar${className ? ` ${className}` : ""}`}
			data-indeterminate={percent === undefined || undefined}
		>
			<div
				className="progress-bar-fill"
				style={percent === undefined ? undefined : { width: `${percent}%` }}
			/>
		</div>
	);
}
