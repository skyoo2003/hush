// SPDX-License-Identifier: GPL-3.0-or-later
import { Slider as RadixSlider } from "radix-ui";
import type { ComponentPropsWithRef } from "react";
import "./Slider.css";

export type SliderProps = Omit<
	ComponentPropsWithRef<typeof RadixSlider.Root>,
	"value" | "defaultValue" | "onValueChange" | "onValueCommit" | "children"
> & {
	/** Accessible name of the thumb. */
	label: string;
	value: number;
	onValueChange: (value: number) => void;
};

export function Slider({
	label,
	value,
	onValueChange,
	className,
	...rest
}: SliderProps) {
	return (
		<RadixSlider.Root
			className={`slider ${className ?? ""}`}
			value={[value]}
			onValueChange={([v]) => {
				if (v !== undefined) onValueChange(v);
			}}
			{...rest}
		>
			<RadixSlider.Track className="slider-track">
				<RadixSlider.Range className="slider-range" />
			</RadixSlider.Track>
			<RadixSlider.Thumb className="slider-thumb" aria-label={label} />
		</RadixSlider.Root>
	);
}
