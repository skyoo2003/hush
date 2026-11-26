// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import {
	type ComponentPropsWithRef,
	type KeyboardEvent,
	type ReactNode,
	useRef,
} from "react";
import "./List.css";

const TYPEAHEAD_RESET_MS = 500;

/** The index an arrow/Home/End key moves to, or null for other keys. No wrap at the ends. */
export function navIndex(
	key: string,
	current: number,
	length: number,
): number | null {
	if (length === 0) return null;
	switch (key) {
		case "ArrowDown":
			return current < 0 ? 0 : Math.min(current + 1, length - 1);
		case "ArrowUp":
			return current < 0 ? 0 : Math.max(current - 1, 0);
		case "Home":
			return 0;
		case "End":
			return length - 1;
		default:
			return null;
	}
}

/** The index type-to-select lands on, or -1. A buffer of one repeated letter cycles through the
 * items starting with it; a longer buffer keeps the current item while it still matches. */
export function typeaheadIndex(
	texts: readonly string[],
	buffer: string,
	current: number,
): number {
	const q = buffer.toLowerCase();
	const first = q.charAt(0);
	const cycle = [...q].every((c) => c === first);
	const prefix = cycle ? first : q;
	const start = cycle ? current + 1 : Math.max(current, 0);
	for (let n = 0; n < texts.length; n++) {
		const i = (start + n) % texts.length;
		if (texts[i]?.toLowerCase().startsWith(prefix)) return i;
	}
	return -1;
}

export type ListProps<T> = Omit<ComponentPropsWithRef<"div">, "children"> & {
	label: string;
	items: readonly T[];
	itemKey(item: T): string;
	/** The text type-to-select matches against. */
	itemText(item: T): string;
	selectedKey: string | null;
	onSelectedKeyChange(key: string): void;
	onOpen?(key: string): void;
	density: "dense" | "two-line";
	children(item: T, state: { selected: boolean }): ReactNode;
};

export function List<T>({
	label,
	items,
	itemKey,
	itemText,
	selectedKey,
	onSelectedKeyChange,
	onOpen,
	density,
	children,
	className,
	...rest
}: ListProps<T>) {
	const buffer = useRef("");
	const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
	const keys = items.map(itemKey);
	const current = selectedKey === null ? -1 : keys.indexOf(selectedKey);
	const tabbable = Math.max(current, 0);

	const select = (listbox: HTMLElement, index: number) => {
		const key = keys[index];
		const option = listbox.children[index];
		if (key === undefined || !(option instanceof HTMLElement)) return;
		onSelectedKeyChange(key);
		option.focus();
		option.scrollIntoView?.({ block: "nearest" });
	};

	const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
		const listbox = e.currentTarget;
		const nav = navIndex(e.key, current, keys.length);
		if (nav !== null) {
			e.preventDefault();
			select(listbox, nav);
			return;
		}
		if (e.key === "Enter") {
			if (selectedKey !== null && current >= 0) {
				e.preventDefault();
				onOpen?.(selectedKey);
			}
			return;
		}
		if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
		e.preventDefault();
		buffer.current += e.key;
		clearTimeout(resetTimer.current);
		resetTimer.current = setTimeout(() => {
			buffer.current = "";
		}, TYPEAHEAD_RESET_MS);
		const match = typeaheadIndex(items.map(itemText), buffer.current, current);
		if (match >= 0) select(listbox, match);
	};

	return (
		<div
			{...rest}
			role="listbox"
			aria-label={label}
			className={`list list-${density}${className ? ` ${className}` : ""}`}
			onKeyDown={onKeyDown}
		>
			{items.map((item, i) => {
				const key = keys[i] ?? "";
				const selected = i === current;
				return (
					// biome-ignore lint/a11y/useKeyWithClickEvents: the listbox handles the keys for every option
					<div
						key={key}
						role="option"
						aria-selected={selected}
						tabIndex={i === tabbable ? 0 : -1}
						className="list-option"
						onClick={(e) =>
							select(e.currentTarget.parentElement ?? e.currentTarget, i)
						}
						onDoubleClick={() => onOpen?.(key)}
					>
						{children(item, { selected })}
					</div>
				);
			})}
		</div>
	);
}

export type ListRowProps = {
	icon?: LucideIcon;
	title: ReactNode;
	subtitle?: ReactNode;
	trailing?: ReactNode;
};

export function ListRow({
	icon: Icon,
	title,
	subtitle,
	trailing,
}: ListRowProps) {
	return (
		<div className="list-row">
			{Icon && <Icon className="list-row-icon" strokeWidth={1.5} aria-hidden />}
			<div className="list-row-text">
				<span className="list-row-title">{title}</span>
				{subtitle !== undefined && (
					<span className="list-row-subtitle">{subtitle}</span>
				)}
			</div>
			{trailing !== undefined && (
				<span className="list-row-trailing">{trailing}</span>
			)}
		</div>
	);
}
