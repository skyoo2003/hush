// SPDX-License-Identifier: GPL-3.0-or-later
import { Dialog } from "radix-ui";
import { type ReactNode, useRef } from "react";
import "./Sheet.css";

type Props = {
	open: boolean;
	onOpenChange(open: boolean): void;
	title: string;
	description?: string;
	children: ReactNode;
	footer?: ReactNode;
};

/** A macOS sheet: slides down from under the toolbar, centered, modal. */
export function Sheet({
	open,
	onOpenChange,
	title,
	description,
	children,
	footer,
}: Props) {
	// Sheets open from anywhere, not a Dialog.Trigger, so Radix has no trigger to refocus on close.
	const opener = useRef<HTMLElement | null>(null);
	return (
		<Dialog.Root open={open} onOpenChange={onOpenChange}>
			<Dialog.Portal>
				<Dialog.Overlay className="sheet-overlay" />
				<Dialog.Content
					className="sheet"
					// Radix warns unless a missing description is opted out explicitly.
					{...(description ? {} : { "aria-describedby": undefined })}
					onOpenAutoFocus={() => {
						opener.current =
							document.activeElement instanceof HTMLElement
								? document.activeElement
								: null;
					}}
					onCloseAutoFocus={(e) => {
						e.preventDefault();
						opener.current?.focus();
					}}
				>
					<Dialog.Title className="sheet-title">{title}</Dialog.Title>
					{description && (
						<Dialog.Description className="sheet-description">
							{description}
						</Dialog.Description>
					)}
					<div className="sheet-body">{children}</div>
					{footer && <div className="sheet-footer">{footer}</div>}
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
