// SPDX-License-Identifier: GPL-3.0-or-later
import { SearchX } from "lucide-react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Trust.css";

type Props = {
	title: string;
	description?: string;
	action?: ReactNode;
} & Omit<ComponentPropsWithRef<"div">, "title" | "children">;

/** "Not found in your local documents": a neutral outcome, never styled as an error. */
export function NotFoundState({
	title,
	description,
	action,
	className,
	...rest
}: Props) {
	return (
		<div role="status" {...rest} className={`not-found ${className ?? ""}`}>
			<SearchX
				className="not-found-icon"
				size={32}
				strokeWidth={1.5}
				aria-hidden
			/>
			<div className="not-found-title">{title}</div>
			{description && (
				<div className="not-found-description">{description}</div>
			)}
			{action}
		</div>
	);
}
