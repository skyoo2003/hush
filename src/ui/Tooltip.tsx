// SPDX-License-Identifier: GPL-3.0-or-later
import { Tooltip as T } from "radix-ui";
import type { ReactElement, ReactNode } from "react";
import "./Popover.css";
import "./Tooltip.css";

type Props = { content: ReactNode; children: ReactElement };

export function Tooltip({ content, children }: Props) {
	return (
		<T.Provider delayDuration={600}>
			<T.Root>
				<T.Trigger asChild>{children}</T.Trigger>
				<T.Portal>
					<T.Content className="tooltip" sideOffset={4} collisionPadding={8}>
						{content}
					</T.Content>
				</T.Portal>
			</T.Root>
		</T.Provider>
	);
}
