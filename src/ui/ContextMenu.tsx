// SPDX-License-Identifier: GPL-3.0-or-later
import { ContextMenu as C } from "radix-ui";
import type { ReactNode } from "react";
import { type MenuEntry, renderMenuEntries } from "./Menu";

type Props = { items: readonly MenuEntry[]; children: ReactNode };

export function ContextMenu({ items, children }: Props) {
	return (
		<C.Root>
			<C.Trigger asChild>{children}</C.Trigger>
			<C.Portal>
				<C.Content className="menu" collisionPadding={8}>
					{renderMenuEntries(items, C)}
				</C.Content>
			</C.Portal>
		</C.Root>
	);
}
