// SPDX-License-Identifier: GPL-3.0-or-later
import { type ComponentPropsWithRef, useId } from "react";
import "./TextField.css";

export type TextFieldProps = ComponentPropsWithRef<"input"> & {
	label?: string;
	description?: string;
	invalid?: boolean;
};

export function TextField({
	label,
	description,
	invalid,
	id,
	className,
	...rest
}: TextFieldProps) {
	const autoId = useId();
	const inputId = id ?? autoId;
	const descriptionId = `${inputId}-description`;
	return (
		<div className={`field ${className ?? ""}`}>
			{label && (
				<label htmlFor={inputId} className="field-label">
					{label}
				</label>
			)}
			<input
				id={inputId}
				className="field-box textfield"
				aria-invalid={invalid || undefined}
				aria-describedby={description ? descriptionId : undefined}
				{...rest}
			/>
			{description && (
				<span id={descriptionId} className="field-description">
					{description}
				</span>
			)}
		</div>
	);
}
