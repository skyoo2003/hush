// SPDX-License-Identifier: GPL-3.0-or-later
import { Check, Minus } from "lucide-react";
import { Checkbox as RadixCheckbox } from "radix-ui";
import { type ComponentPropsWithRef, useId } from "react";
import "./Switch.css";
import "./Checkbox.css";

export type CheckboxProps = Omit<
	ComponentPropsWithRef<typeof RadixCheckbox.Root>,
	"children"
> & {
	label: string;
};

/** `checked="indeterminate"` draws the mixed state. */
export function Checkbox({ label, id, className, ...rest }: CheckboxProps) {
	const autoId = useId();
	const boxId = id ?? autoId;
	return (
		<span className={`toggle-row ${className ?? ""}`}>
			<RadixCheckbox.Root id={boxId} className="checkbox" {...rest}>
				<RadixCheckbox.Indicator className="checkbox-indicator">
					<Check
						className="checkbox-check"
						size={12}
						strokeWidth={2.5}
						aria-hidden
					/>
					<Minus
						className="checkbox-mixed"
						size={12}
						strokeWidth={2.5}
						aria-hidden
					/>
				</RadixCheckbox.Indicator>
			</RadixCheckbox.Root>
			<label htmlFor={boxId} className="toggle-label">
				{label}
			</label>
		</span>
	);
}
