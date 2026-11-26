// SPDX-License-Identifier: GPL-3.0-or-later
import { ChevronDown, ChevronUp } from "lucide-react";
import { type ComponentPropsWithRef, useState } from "react";
import type { ControlSize } from "./Button";
import "./TextField.css";
import "./Stepper.css";

export type StepperProps = Omit<
	ComponentPropsWithRef<"input">,
	| "value"
	| "defaultValue"
	| "onChange"
	| "min"
	| "max"
	| "step"
	| "size"
	| "type"
> & {
	/** Accessible name of the number field. */
	label: string;
	incrementLabel: string;
	decrementLabel: string;
	value: number;
	onValueChange: (value: number) => void;
	min: number;
	max: number;
	step?: number;
	size?: ControlSize;
};

/** A number field with the macOS up/down arrow pair. Typed text commits on Enter or blur. */
export function Stepper({
	label,
	incrementLabel,
	decrementLabel,
	value,
	onValueChange,
	min,
	max,
	step = 1,
	size = "regular",
	disabled,
	className,
	onKeyDown,
	onBlur,
	...rest
}: StepperProps) {
	const [draft, setDraft] = useState<string | null>(null);
	const clamp = (n: number) => Math.min(max, Math.max(min, n));
	const set = (n: number) => {
		setDraft(null);
		const next = clamp(n);
		if (next !== value) onValueChange(next);
	};
	const commit = () => {
		if (draft === null) return;
		const n = Number(draft);
		// Empty or non-numeric text reverts to the last value.
		if (draft.trim() === "" || !Number.isFinite(n)) setDraft(null);
		else set(n);
	};
	return (
		<span className={`stepper stepper-${size} ${className ?? ""}`}>
			<input
				type="text"
				inputMode="decimal"
				role="spinbutton"
				aria-label={label}
				aria-valuemin={min}
				aria-valuemax={max}
				aria-valuenow={value}
				className="field-box textfield stepper-input"
				value={draft ?? String(value)}
				disabled={disabled}
				onChange={(e) => setDraft(e.target.value)}
				onBlur={(e) => {
					commit();
					onBlur?.(e);
				}}
				onKeyDown={(e) => {
					onKeyDown?.(e);
					if (e.defaultPrevented) return;
					if (e.key === "ArrowUp" || e.key === "ArrowDown") {
						e.preventDefault();
						set(value + (e.key === "ArrowUp" ? step : -step));
					} else if (e.key === "Enter") {
						commit();
					}
				}}
				{...rest}
			/>
			<span className="stepper-arrows">
				{/* Not in the tab order: ↑/↓ on the field do the same, as on macOS. */}
				<button
					type="button"
					tabIndex={-1}
					className="stepper-arrow"
					aria-label={incrementLabel}
					disabled={disabled || value >= max}
					onClick={() => set(value + step)}
				>
					<ChevronUp size={10} strokeWidth={2} aria-hidden />
				</button>
				<button
					type="button"
					tabIndex={-1}
					className="stepper-arrow"
					aria-label={decrementLabel}
					disabled={disabled || value <= min}
					onClick={() => set(value - step)}
				>
					<ChevronDown size={10} strokeWidth={2} aria-hidden />
				</button>
			</span>
		</span>
	);
}
