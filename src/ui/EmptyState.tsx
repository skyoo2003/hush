// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import { Text } from "./Text";
import "./EmptyState.css";

export type EmptyStateProps = Omit<
	ComponentPropsWithRef<"div">,
	"children" | "title"
> & {
	icon: LucideIcon;
	title: string;
	description?: string;
	action?: ReactNode;
};

export function EmptyState({
	icon: Icon,
	title,
	description,
	action,
	className,
	...rest
}: EmptyStateProps) {
	return (
		<div {...rest} className={`empty-state${className ? ` ${className}` : ""}`}>
			<Icon
				className="empty-state-icon"
				size={32}
				strokeWidth={1.5}
				aria-hidden
			/>
			<Text variant="title3" as="p">
				{title}
			</Text>
			{description && (
				<Text tone="secondary" as="p">
					{description}
				</Text>
			)}
			{action}
		</div>
	);
}
