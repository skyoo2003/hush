// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import type { ComponentPropsWithRef } from "react";
import type { ControlSize } from "./Button";
import "./Button.css";
import "./IconButton.css";

export type IconButtonProps = Omit<
	ComponentPropsWithRef<"button">,
	"children" | "aria-label"
> & {
	/** The accessible name; there is no visible text. */
	label: string;
	icon: LucideIcon;
	variant?: "plain" | "secondary";
	size?: ControlSize;
};

export function IconButton({
	label,
	icon: Icon,
	variant = "plain",
	size = "regular",
	type = "button",
	className,
	...rest
}: IconButtonProps) {
	return (
		<button
			type={type}
			aria-label={label}
			className={`btn btn-${variant} btn-${size} icon-btn ${className ?? ""}`}
			{...rest}
		>
			<Icon size={size === "small" ? 14 : 16} strokeWidth={1.5} aria-hidden />
		</button>
	);
}
