// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Alert } from "./Alert";

function renderAlert(tone: "default" | "destructive") {
	const onConfirm = vi.fn();
	const onOpenChange = vi.fn();
	render(
		<Alert
			open
			onOpenChange={onOpenChange}
			title="Remove folder?"
			message="Files on disk stay."
			confirmLabel="Remove"
			cancelLabel="Cancel"
			onConfirm={onConfirm}
			tone={tone}
		/>,
	);
	return { onConfirm, onOpenChange };
}

describe("Alert", () => {
	it("focuses Cancel first when destructive", () => {
		renderAlert("destructive");
		expect(
			screen.getByRole("alertdialog", { name: "Remove folder?" }),
		).toBeTruthy();
		expect(document.activeElement).toBe(
			screen.getByRole("button", { name: "Cancel" }),
		);
	});

	it("focuses the confirm button first by default", () => {
		renderAlert("default");
		expect(document.activeElement).toBe(
			screen.getByRole("button", { name: "Remove" }),
		);
	});

	it("calls onConfirm and closes when confirmed", async () => {
		const user = userEvent.setup();
		const { onConfirm, onOpenChange } = renderAlert("destructive");
		await user.click(screen.getByRole("button", { name: "Remove" }));
		expect(onConfirm).toHaveBeenCalledTimes(1);
		expect(onOpenChange).toHaveBeenCalledWith(false);
	});
});
