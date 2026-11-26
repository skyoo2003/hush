// SPDX-License-Identifier: GPL-3.0-or-later
import { Search, X } from "lucide-react";
import {
	type ComponentPropsWithRef,
	type ReactNode,
	useImperativeHandle,
	useRef,
} from "react";
import type { ControlSize } from "./Button";
import "./TextField.css";
import "./SearchField.css";

export type SearchFieldProps = Omit<
	ComponentPropsWithRef<"input">,
	"type" | "value" | "onChange" | "size"
> & {
	value: string;
	onChange: (value: string) => void;
	/** Accessible name of the clear button. */
	clearLabel: string;
	/** Filter chips (usually Tag) shown inside the field before the text. */
	chips?: ReactNode;
	size?: ControlSize;
};

export function SearchField({
	value,
	onChange,
	clearLabel,
	chips,
	size = "regular",
	className,
	onKeyDown,
	ref,
	...rest
}: SearchFieldProps) {
	const input = useRef<HTMLInputElement>(null);
	useImperativeHandle(ref, () => input.current as HTMLInputElement);
	const clear = () => {
		onChange("");
		input.current?.focus();
	};
	return (
		<div
			className={`field-box searchfield searchfield-${size} ${className ?? ""}`}
		>
			<Search
				className="searchfield-icon"
				size={size === "small" ? 12 : 14}
				strokeWidth={1.5}
				aria-hidden
			/>
			{chips}
			<input
				ref={input}
				type="search"
				className="searchfield-input"
				value={value}
				onChange={(e) => onChange(e.target.value)}
				onKeyDown={(e) => {
					onKeyDown?.(e);
					if (e.key === "Escape" && value !== "" && !e.defaultPrevented) {
						// Esc first clears; only an empty field lets Esc reach a popup or window.
						e.stopPropagation();
						clear();
					}
				}}
				{...rest}
			/>
			{value !== "" && (
				<button
					type="button"
					className="searchfield-clear"
					aria-label={clearLabel}
					disabled={rest.disabled}
					onClick={clear}
				>
					<X size={12} strokeWidth={2} aria-hidden />
				</button>
			)}
		</div>
	);
}
