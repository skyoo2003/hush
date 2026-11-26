// SPDX-License-Identifier: GPL-3.0-or-later
import { Popover as P } from "radix-ui";
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Popover.css";

type Props = {
	trigger: ReactNode;
	open?: boolean;
	onOpenChange?(open: boolean): void;
	arrow?: boolean;
	children: ReactNode;
} & Omit<ComponentPropsWithRef<typeof P.Content>, "children">;

export function Popover({
	trigger,
	open,
	onOpenChange,
	arrow,
	children,
	className,
	...rest
}: Props) {
	return (
		<P.Root open={open} onOpenChange={onOpenChange}>
			<P.Trigger asChild>{trigger}</P.Trigger>
			<P.Portal>
				<P.Content
					sideOffset={6}
					collisionPadding={8}
					{...rest}
					className={`popover ${className ?? ""}`}
				>
					{children}
					{arrow && <P.Arrow className="popover-arrow" width={12} height={6} />}
				</P.Content>
			</P.Portal>
		</P.Root>
	);
}
