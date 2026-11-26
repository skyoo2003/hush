// SPDX-License-Identifier: GPL-3.0-or-later
import { Check, ChevronsUpDown } from "lucide-react";
import { Select as RadixSelect } from "radix-ui";
import type { ComponentPropsWithRef } from "react";
import type { ControlSize } from "./Button";
import "./Button.css";
import "./Select.css";

export type SelectProps<V extends string> = Omit<
	ComponentPropsWithRef<typeof RadixSelect.Trigger>,
	"children" | "aria-label" | "value" | "disabled"
> & {
	label: string;
	options: readonly { value: V; label: string; disabled?: boolean }[];
	value: V;
	onValueChange: (value: V) => void;
	size?: ControlSize;
	disabled?: boolean;
};

/** A macOS pop-up button: the menu opens over the trigger with the current item in place. */
export function Select<V extends string>({
	label,
	options,
	value,
	onValueChange,
	size = "regular",
	disabled,
	className,
	...rest
}: SelectProps<V>) {
	return (
		<RadixSelect.Root
			value={value}
			onValueChange={(v) => {
				const option = options.find((o) => o.value === v);
				if (option) onValueChange(option.value);
			}}
			disabled={disabled}
		>
			<RadixSelect.Trigger
				aria-label={label}
				className={`btn btn-secondary btn-${size} select-trigger ${className ?? ""}`}
				{...rest}
			>
				<RadixSelect.Value />
				<RadixSelect.Icon className="select-chevrons">
					<ChevronsUpDown
						size={size === "small" ? 12 : 14}
						strokeWidth={1.5}
						aria-hidden
					/>
				</RadixSelect.Icon>
			</RadixSelect.Trigger>
			<RadixSelect.Portal>
				<RadixSelect.Content
					position="item-aligned"
					className={`select-menu select-menu-${size}`}
				>
					<RadixSelect.Viewport>
						{options.map((o) => (
							<RadixSelect.Item
								key={o.value}
								value={o.value}
								disabled={o.disabled}
								className="select-item"
							>
								<RadixSelect.ItemIndicator className="select-check">
									<Check size={14} strokeWidth={2} aria-hidden />
								</RadixSelect.ItemIndicator>
								<RadixSelect.ItemText>{o.label}</RadixSelect.ItemText>
							</RadixSelect.Item>
						))}
					</RadixSelect.Viewport>
				</RadixSelect.Content>
			</RadixSelect.Portal>
		</RadixSelect.Root>
	);
}
