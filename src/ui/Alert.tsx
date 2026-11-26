// SPDX-License-Identifier: GPL-3.0-or-later
import type { LucideIcon } from "lucide-react";
import { AlertDialog } from "radix-ui";
import { useRef } from "react";
import { Button } from "./Button";
import "./Alert.css";

type Props = {
	open: boolean;
	onOpenChange(open: boolean): void;
	title: string;
	message?: string;
	icon?: LucideIcon;
	confirmLabel: string;
	cancelLabel: string;
	onConfirm(): void;
	tone: "default" | "destructive";
};

/** A centered macOS alert. Return confirms by default; a destructive one starts on Cancel. */
export function Alert({
	open,
	onOpenChange,
	title,
	message,
	icon: Icon,
	confirmLabel,
	cancelLabel,
	onConfirm,
	tone,
}: Props) {
	const confirmRef = useRef<HTMLButtonElement>(null);
	// Alerts open from anywhere, not an AlertDialog.Trigger, so Radix has no trigger to refocus.
	const opener = useRef<HTMLElement | null>(null);
	return (
		<AlertDialog.Root open={open} onOpenChange={onOpenChange}>
			<AlertDialog.Portal>
				<AlertDialog.Overlay className="alert-overlay" />
				<AlertDialog.Content
					className="alert"
					{...(message ? {} : { "aria-describedby": undefined })}
					onOpenAutoFocus={(e) => {
						opener.current =
							document.activeElement instanceof HTMLElement
								? document.activeElement
								: null;
						// Radix focuses Cancel; only a destructive alert keeps that.
						if (tone === "default") {
							e.preventDefault();
							confirmRef.current?.focus();
						}
					}}
					onCloseAutoFocus={(e) => {
						e.preventDefault();
						opener.current?.focus();
					}}
				>
					{Icon && (
						<Icon
							className="alert-icon"
							size={48}
							strokeWidth={1.5}
							aria-hidden
						/>
					)}
					<AlertDialog.Title className="alert-title">{title}</AlertDialog.Title>
					{message && (
						<AlertDialog.Description className="alert-message">
							{message}
						</AlertDialog.Description>
					)}
					<div className="alert-buttons">
						<AlertDialog.Action asChild>
							<Button
								ref={confirmRef}
								variant={tone === "destructive" ? "destructive" : "primary"}
								onClick={onConfirm}
							>
								{confirmLabel}
							</Button>
						</AlertDialog.Action>
						<AlertDialog.Cancel asChild>
							<Button variant="secondary">{cancelLabel}</Button>
						</AlertDialog.Cancel>
					</div>
				</AlertDialog.Content>
			</AlertDialog.Portal>
		</AlertDialog.Root>
	);
}
