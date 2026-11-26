// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Sheet } from "./Sheet";

function Harness({ onOpenChange }: { onOpenChange(open: boolean): void }) {
	const [open, setOpen] = useState(false);
	return (
		<>
			<button type="button" onClick={() => setOpen(true)}>
				Opener
			</button>
			<Sheet
				open={open}
				onOpenChange={(v) => {
					onOpenChange(v);
					setOpen(v);
				}}
				title="Sheet title"
				footer={<button type="button">Last</button>}
			>
				<input aria-label="First" />
			</Sheet>
		</>
	);
}

describe("Sheet", () => {
	it("traps focus, closes on Escape, and returns focus to the opener", async () => {
		const user = userEvent.setup();
		const onOpenChange = vi.fn();
		render(<Harness onOpenChange={onOpenChange} />);
		const opener = screen.getByRole("button", { name: "Opener" });
		opener.focus();
		await user.keyboard("{Enter}");

		const dialog = screen.getByRole("dialog", { name: "Sheet title" });
		const first = screen.getByRole("textbox", { name: "First" });
		const last = screen.getByRole("button", { name: "Last" });
		expect(document.activeElement).toBe(first);
		expect(dialog.contains(document.activeElement)).toBe(true);

		await user.tab();
		expect(document.activeElement).toBe(last);
		await user.tab();
		expect(document.activeElement).toBe(first);
		await user.tab({ shift: true });
		expect(document.activeElement).toBe(last);

		await user.keyboard("{Escape}");
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(screen.queryByRole("dialog")).toBeNull();
		// Radix restores focus in a timeout after the focus scope unmounts.
		await waitFor(() => expect(document.activeElement).toBe(opener));
	});
});
