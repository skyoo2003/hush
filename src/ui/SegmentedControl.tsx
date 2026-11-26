// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import { RadioGroup } from "radix-ui";
import type { ComponentPropsWithRef } from "react";
import type { ControlSize } from "./Button";
import "./SegmentedControl.css";

export type SegmentedControlProps<V extends string> = Omit<
	ComponentPropsWithRef<typeof RadioGroup.Root>,
	| "children"
	| "aria-label"
	| "value"
	| "defaultValue"
	| "onValueChange"
	| "orientation"
> & {
	label: string;
	options: readonly { value: V; label: string; icon?: LucideIcon }[];
	value: V;
	onValueChange: (value: V) => void;
	size?: ControlSize;
};

export function SegmentedControl<V extends string>({
	label,
	options,
	value,
	onValueChange,
	size = "regular",
	className,
	...rest
}: SegmentedControlProps<V>) {
	return (
		<RadioGroup.Root
			aria-label={label}
			orientation="horizontal"
			value={value}
			onValueChange={(v) => {
				const option = options.find((o) => o.value === v);
				if (option) onValueChange(option.value);
			}}
			className={`segmented segmented-${size} ${className ?? ""}`}
			{...rest}
		>
			{options.map(({ value: v, label: text, icon: Icon }) => (
				<RadioGroup.Item key={v} value={v} className="segment">
					{Icon && (
						<Icon
							size={size === "small" ? 14 : 16}
							strokeWidth={1.5}
							aria-hidden
						/>
					)}
					{text}
				</RadioGroup.Item>
			))}
		</RadioGroup.Root>
	);
}
