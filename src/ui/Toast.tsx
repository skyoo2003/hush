// SPDX-License-Identifier: GPL-3.0-or-later
import { X } from "lucide-react";
import { Toast as T } from "radix-ui";
import type { ReactNode } from "react";
import "./Toast.css";

/** Wrap the app once; `label` names the notification region for screen readers. */
export function ToastProvider({
	label,
	children,
}: {
	label: string;
	children: ReactNode;
}) {
	return (
		<T.Provider duration={4000} label={label}>
			{children}
			<T.Viewport className="toast-viewport" label={label} />
		</T.Provider>
	);
}

type Props = {
	open: boolean;
	onOpenChange(open: boolean): void;
	title: string;
	description?: string;
	action?: { label: string; altText: string; onClick(): void };
	closeLabel: string;
};

export function Toast({
	open,
	onOpenChange,
	title,
	description,
	action,
	closeLabel,
}: Props) {
	return (
		<T.Root className="toast" open={open} onOpenChange={onOpenChange}>
			<div className="toast-text">
				<T.Title className="toast-title">{title}</T.Title>
				{description && (
					<T.Description className="toast-description">
						{description}
					</T.Description>
				)}
			</div>
			{action && (
				<T.Action
					className="toast-action"
					altText={action.altText}
					onClick={action.onClick}
				>
					{action.label}
				</T.Action>
			)}
			<T.Close className="toast-close" aria-label={closeLabel}>
				<X size={14} strokeWidth={1.5} aria-hidden />
			</T.Close>
		</T.Root>
	);
}
