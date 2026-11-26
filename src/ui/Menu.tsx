// SPDX-License-Identifier: GPL-3.0-or-later
import { Check, type LucideIcon } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import type { ReactNode } from "react";
import "./Popover.css";
import "./Menu.css";

export type MenuEntry =
	| {
			type: "item";
			id: string;
			label: string;
			icon?: LucideIcon;
			shortcut?: ReactNode;
			disabled?: boolean;
			destructive?: boolean;
			onSelect(): void;
	  }
	| { type: "separator"; id: string }
	| {
			type: "checkbox";
			id: string;
			label: string;
			checked: boolean;
			onCheckedChange(checked: boolean): void;
	  };

/** The Radix parts both menus share; DropdownMenu and ContextMenu expose the same shapes. */
export type MenuParts = Pick<
	typeof DropdownMenu,
	"Item" | "Separator" | "CheckboxItem" | "ItemIndicator"
>;

/** The one item renderer for Menu and ContextMenu. */
export function renderMenuEntries(
	entries: readonly MenuEntry[],
	P: MenuParts,
): ReactNode {
	return entries.map((e) => {
		if (e.type === "separator")
			return <P.Separator key={e.id} className="menu-separator" />;
		if (e.type === "checkbox")
			return (
				<P.CheckboxItem
					key={e.id}
					className="menu-item"
					checked={e.checked}
					onCheckedChange={(v) => e.onCheckedChange(v === true)}
				>
					<span className="menu-icon">
						<P.ItemIndicator>
							<Check size={14} strokeWidth={1.5} />
						</P.ItemIndicator>
					</span>
					<span className="menu-label">{e.label}</span>
				</P.CheckboxItem>
			);
		const Icon = e.icon;
		return (
			<P.Item
				key={e.id}
				className="menu-item"
				disabled={e.disabled}
				data-destructive={e.destructive || undefined}
				onSelect={e.onSelect}
			>
				<span className="menu-icon">
					{Icon && <Icon size={14} strokeWidth={1.5} />}
				</span>
				<span className="menu-label">{e.label}</span>
				{e.shortcut != null && (
					<kbd className="menu-shortcut">{e.shortcut}</kbd>
				)}
			</P.Item>
		);
	});
}

type Props = { items: readonly MenuEntry[]; trigger: ReactNode };

export function Menu({ items, trigger }: Props) {
	return (
		<DropdownMenu.Root>
			<DropdownMenu.Trigger asChild>{trigger}</DropdownMenu.Trigger>
			<DropdownMenu.Portal>
				<DropdownMenu.Content
					className="menu"
					sideOffset={4}
					align="start"
					collisionPadding={8}
				>
					{renderMenuEntries(items, DropdownMenu)}
				</DropdownMenu.Content>
			</DropdownMenu.Portal>
		</DropdownMenu.Root>
	);
}
