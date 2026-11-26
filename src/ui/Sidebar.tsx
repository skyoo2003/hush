// SPDX-License-Identifier: GPL-3.0-or-later
import { ChevronRight, type LucideIcon } from "lucide-react";
import { Collapsible } from "radix-ui";
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Sidebar.css";

export type SidebarItem = {
	id: string;
	label: string;
	icon?: LucideIcon;
	count?: number;
	badge?: ReactNode;
};

export type SidebarSection = {
	id: string;
	title?: string;
	collapsible?: boolean;
	items: readonly SidebarItem[];
};

export type SidebarProps = Omit<ComponentPropsWithRef<"nav">, "onSelect"> & {
	label: string;
	sections: readonly SidebarSection[];
	selectedId: string | null;
	onSelect(id: string): void;
};

export function Sidebar({
	label,
	sections,
	selectedId,
	onSelect,
	className,
	...rest
}: SidebarProps) {
	return (
		<nav
			{...rest}
			aria-label={label}
			className={`sidebar${className ? ` ${className}` : ""}`}
		>
			{sections.map((section) => {
				const items = (
					<ul className="sidebar-items">
						{section.items.map(({ id, label, icon: Icon, count, badge }) => (
							<li key={id}>
								<button
									type="button"
									className="sidebar-item"
									aria-current={id === selectedId ? "page" : undefined}
									onClick={() => onSelect(id)}
								>
									{Icon && (
										<Icon
											className="sidebar-item-icon"
											size={16}
											strokeWidth={1.5}
											aria-hidden
										/>
									)}
									<span className="sidebar-item-label">{label}</span>
									{badge}
									{count !== undefined && (
										<span className="sidebar-item-count">{count}</span>
									)}
								</button>
							</li>
						))}
					</ul>
				);
				if (!section.collapsible || !section.title) {
					return (
						<section key={section.id} className="sidebar-section">
							{section.title && (
								<div className="sidebar-section-title">{section.title}</div>
							)}
							{items}
						</section>
					);
				}
				return (
					<Collapsible.Root
						key={section.id}
						defaultOpen
						className="sidebar-section"
					>
						<Collapsible.Trigger className="sidebar-section-title sidebar-section-trigger">
							<ChevronRight
								className="sidebar-chevron"
								size={12}
								strokeWidth={2}
								aria-hidden
							/>
							{section.title}
						</Collapsible.Trigger>
						<Collapsible.Content>{items}</Collapsible.Content>
					</Collapsible.Root>
				);
			})}
		</nav>
	);
}
