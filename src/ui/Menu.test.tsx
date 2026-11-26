// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Menu, type MenuEntry } from "./Menu";

function setup() {
	const open = vi.fn();
	const remove = vi.fn();
	const items: MenuEntry[] = [
		{ type: "item", id: "open", label: "Open", onSelect: open },
		{
			type: "item",
			id: "dis",
			label: "Disabled",
			disabled: true,
			onSelect: remove,
		},
		{ type: "separator", id: "s" },
		{
			type: "item",
			id: "rm",
			label: "Remove",
			destructive: true,
			onSelect: remove,
		},
	];
	render(<Menu items={items} trigger={<button type="button">More</button>} />);
	return {
		user: userEvent.setup(),
		open,
		remove,
		trigger: screen.getByRole("button", { name: "More" }),
	};
}

describe("Menu", () => {
	it("opens on click and on Enter", async () => {
		const { user, trigger } = setup();
		await user.click(trigger);
		expect(screen.getByRole("menu")).toBeTruthy();
		await user.keyboard("{Escape}");
		expect(screen.queryByRole("menu")).toBeNull();
		trigger.focus();
		await user.keyboard("{Enter}");
		expect(screen.getByRole("menu")).toBeTruthy();
	});

	it("moves the highlight with ArrowDown, skips disabled, and selects with Enter", async () => {
		const { user, trigger, open, remove } = setup();
		trigger.focus();
		await user.keyboard("{Enter}");
		expect(document.activeElement).toBe(
			screen.getByRole("menuitem", { name: "Open" }),
		);
		await user.keyboard("{ArrowDown}");
		const rm = screen.getByRole("menuitem", { name: "Remove" });
		expect(document.activeElement).toBe(rm);
		expect(rm.hasAttribute("data-highlighted")).toBe(true);
		await user.keyboard("{Enter}");
		expect(remove).toHaveBeenCalledTimes(1);
		expect(open).not.toHaveBeenCalled();
		expect(screen.queryByRole("menu")).toBeNull();
	});

	it("does not select a disabled item", async () => {
		const { user, trigger, remove } = setup();
		await user.click(trigger);
		const dis = screen.getByRole("menuitem", { name: "Disabled" });
		expect(dis.getAttribute("aria-disabled")).toBe("true");
		await user.click(dis);
		expect(remove).not.toHaveBeenCalled();
		expect(screen.getByRole("menu")).toBeTruthy();
	});

	it("closes on Escape and returns focus to the trigger", async () => {
		const { user, trigger } = setup();
		trigger.focus();
		await user.keyboard("{Enter}");
		await user.keyboard("{Escape}");
		expect(screen.queryByRole("menu")).toBeNull();
		expect(document.activeElement).toBe(trigger);
	});
});
