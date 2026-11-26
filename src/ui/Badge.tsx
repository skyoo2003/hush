// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef } from "react";
import "./Badge.css";

export type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

export type BadgeProps = ComponentPropsWithRef<"span"> & { tone?: BadgeTone };

export function Badge({ tone = "neutral", className, ...rest }: BadgeProps) {
	return (
		<span className={`badge badge-${tone} ${className ?? ""}`} {...rest} />
	);
}
