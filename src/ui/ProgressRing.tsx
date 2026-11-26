// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef } from "react";
import "./ProgressRing.css";

const RADIUS = 6;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

export type ProgressRingProps = Omit<
	ComponentPropsWithRef<"span">,
	"children"
> & {
	label: string;
	/** 0–1; undefined is indeterminate. */
	value?: number;
};

export function ProgressRing({
	label,
	value,
	className,
	...rest
}: ProgressRingProps) {
	const fraction =
		value === undefined ? undefined : Math.min(1, Math.max(0, value));
	return (
		<span
			{...rest}
			role="progressbar"
			aria-label={label}
			aria-valuemin={0}
			aria-valuemax={100}
			aria-valuenow={
				fraction === undefined ? undefined : Math.round(fraction * 100)
			}
			className={`progress-ring${className ? ` ${className}` : ""}`}
			data-indeterminate={fraction === undefined || undefined}
		>
			<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden>
				<circle className="progress-ring-track" cx="8" cy="8" r={RADIUS} />
				<circle
					className="progress-ring-arc"
					cx="8"
					cy="8"
					r={RADIUS}
					strokeDasharray={CIRCUMFERENCE}
					strokeDashoffset={CIRCUMFERENCE * (1 - (fraction ?? 0.25))}
				/>
			</svg>
		</span>
	);
}
