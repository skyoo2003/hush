// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import "./Button.css";

export type ControlSize = "regular" | "small";
export type ButtonVariant = "primary" | "secondary" | "plain" | "destructive";

export type ButtonProps = ComponentPropsWithRef<"button"> & {
	variant?: ButtonVariant;
	size?: ControlSize;
	icon?: LucideIcon;
};

export function Button({
	variant = "secondary",
	size = "regular",
	icon: Icon,
	type = "button",
	className,
	children,
	...rest
}: ButtonProps) {
	return (
		<button
			type={type}
			className={`btn btn-${variant} btn-${size} ${className ?? ""}`}
			{...rest}
		>
			{Icon && (
				<Icon size={size === "small" ? 14 : 16} strokeWidth={1.5} aria-hidden />
			)}
			{children}
		</button>
	);
}
