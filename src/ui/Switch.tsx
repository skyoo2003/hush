// SPDX-License-Identifier: GPL-3.0-or-later
import { Switch as RadixSwitch } from "radix-ui";
import { type ComponentPropsWithRef, useId } from "react";
import type { ControlSize } from "./Button";
import "./Switch.css";

export type SwitchProps = Omit<
	ComponentPropsWithRef<typeof RadixSwitch.Root>,
	"children"
> & {
	label: string;
	/** `small` is the macOS mini switch. */
	size?: ControlSize;
};

export function Switch({
	label,
	size = "regular",
	id,
	className,
	...rest
}: SwitchProps) {
	const autoId = useId();
	const switchId = id ?? autoId;
	return (
		<span className={`toggle-row ${className ?? ""}`}>
			<label htmlFor={switchId} className="toggle-label">
				{label}
			</label>
			<RadixSwitch.Root
				id={switchId}
				className={`switch switch-${size}`}
				{...rest}
			>
				<RadixSwitch.Thumb className="switch-thumb" />
			</RadixSwitch.Root>
		</span>
	);
}
