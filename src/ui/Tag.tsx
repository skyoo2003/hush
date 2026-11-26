// SPDX-License-Identifier: GPL-3.0-or-later
import { X } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Tag.css";

export type TagProps = Omit<ComponentPropsWithRef<"span">, "children"> & {
	label: string;
} & (
		| { onRemove?: undefined; removeLabel?: undefined }
		| { onRemove: () => void; removeLabel: string }
	);

/** A filter chip, e.g. inside SearchField's `chips` slot. */
export function Tag({
	label,
	onRemove,
	removeLabel,
	className,
	...rest
}: TagProps) {
	return (
		<span className={`tag ${className ?? ""}`} {...rest}>
			{label}
			{onRemove && (
				<button
					type="button"
					className="tag-remove"
					aria-label={removeLabel}
					onClick={onRemove}
				>
					<X size={10} strokeWidth={2} aria-hidden />
				</button>
			)}
		</span>
	);
}
