// SPDX-License-Identifier: GPL-3.0-or-later
import {
	type ComponentPropsWithRef,
	type KeyboardEvent,
	type PointerEvent,
	type ReactNode,
	useRef,
} from "react";
import "./SplitView.css";

const KEY_STEP = 8;

export type SplitViewProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
	/** The leading pane width in px. */
	size: number;
	onSizeChange(size: number): void;
	min?: number;
	max?: number;
	dividerLabel: string;
	leading: ReactNode;
	trailing: ReactNode;
};

export function SplitView({
	size,
	onSizeChange,
	min = 180,
	max = 300,
	dividerLabel,
	leading,
	trailing,
	className,
	style,
	...rest
}: SplitViewProps) {
	const drag = useRef<{ x: number; size: number } | null>(null);
	const resize = (next: number) =>
		onSizeChange(Math.min(max, Math.max(min, Math.round(next))));

	const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
		const step =
			e.key === "ArrowRight" ? KEY_STEP : e.key === "ArrowLeft" ? -KEY_STEP : 0;
		if (step === 0) return;
		e.preventDefault();
		resize(size + step);
	};
	const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
		e.currentTarget.setPointerCapture(e.pointerId);
		drag.current = { x: e.clientX, size };
	};
	const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
		if (drag.current) resize(drag.current.size + e.clientX - drag.current.x);
	};
	const endDrag = () => {
		drag.current = null;
	};

	return (
		<div
			{...rest}
			className={`split-view${className ? ` ${className}` : ""}`}
			style={{ ...style, gridTemplateColumns: `${size}px 1px minmax(0, 1fr)` }}
		>
			<div className="split-view-pane">{leading}</div>
			{/* biome-ignore lint/a11y/useSemanticElements: a focusable, resizable separator has no native element */}
			<div
				role="separator"
				aria-orientation="vertical"
				aria-label={dividerLabel}
				aria-valuenow={size}
				aria-valuemin={min}
				aria-valuemax={max}
				tabIndex={0}
				className="split-view-divider"
				onKeyDown={onKeyDown}
				onPointerDown={onPointerDown}
				onPointerMove={onPointerMove}
				onPointerUp={endDrag}
				onPointerCancel={endDrag}
			/>
			<div className="split-view-pane">{trailing}</div>
		</div>
	);
}
