// SPDX-License-Identifier: GPL-3.0-or-later
import { CircleCheck, CircleX, Info, TriangleAlert, X } from "lucide-react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Banner.css";

const ICONS = {
	info: Info,
	success: CircleCheck,
	warning: TriangleAlert,
	error: CircleX,
} as const;

type Props = {
	tone: keyof typeof ICONS;
	title?: string;
	children: ReactNode;
	action?: ReactNode;
} & (
	| { onDismiss?: undefined; dismissLabel?: undefined }
	| { onDismiss(): void; dismissLabel: string }
) &
	Omit<ComponentPropsWithRef<"div">, "title" | "children">;

export function Banner({
	tone,
	title,
	children,
	action,
	onDismiss,
	dismissLabel,
	className,
	...rest
}: Props) {
	const Icon = ICONS[tone];
	return (
		<div
			role={tone === "error" ? "alert" : "status"}
			{...rest}
			className={`banner banner-${tone} ${className ?? ""}`}
		>
			<Icon className="banner-icon" size={16} strokeWidth={1.5} aria-hidden />
			<div className="banner-text">
				{title && <div className="banner-title">{title}</div>}
				<div>{children}</div>
			</div>
			{action && <div className="banner-action">{action}</div>}
			{onDismiss && (
				<button
					type="button"
					className="banner-dismiss"
					aria-label={dismissLabel}
					onClick={onDismiss}
				>
					<X size={14} strokeWidth={1.5} aria-hidden />
				</button>
			)}
		</div>
	);
}
