// SPDX-License-Identifier: GPL-3.0-or-later
import type { ComponentPropsWithRef, CSSProperties } from "react";
import "./Skeleton.css";

export type SkeletonProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
	width?: CSSProperties["width"];
	height?: CSSProperties["height"];
	/** Renders text-line placeholders instead of one block; the last line is shorter. */
	lines?: number;
};

export function Skeleton({
	width,
	height,
	lines,
	className,
	style,
	...rest
}: SkeletonProps) {
	const cls = className ? ` ${className}` : "";
	if (lines === undefined) {
		return (
			<div
				{...rest}
				className={`skeleton${cls}`}
				aria-hidden
				style={{ ...style, width, height }}
			/>
		);
	}
	return (
		<div
			{...rest}
			className={`skeleton-lines${cls}`}
			aria-hidden
			style={{ ...style, width }}
		>
			{Array.from({ length: lines }, (_, i) => (
				<div
					// biome-ignore lint/suspicious/noArrayIndexKey: static placeholders with no identity
					key={i}
					className="skeleton skeleton-line"
					style={{
						height,
						width: i === lines - 1 && lines > 1 ? "60%" : undefined,
					}}
				/>
			))}
		</div>
	);
}
