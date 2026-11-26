// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SearchField } from "./SearchField";

afterEach(cleanup);

describe("SearchField", () => {
	it("shows the clear button only when there is a value", () => {
		const { rerender } = render(
			<SearchField
				aria-label="Search"
				clearLabel="Clear"
				value=""
				onChange={() => {}}
			/>,
		);
		expect(screen.queryByRole("button", { name: "Clear" })).toBeNull();
		rerender(
			<SearchField
				aria-label="Search"
				clearLabel="Clear"
				value="q"
				onChange={() => {}}
			/>,
		);
		expect(screen.getByRole("button", { name: "Clear" })).toBeTruthy();
	});

	it("clear empties the value and refocuses the field", async () => {
		const user = userEvent.setup();
		const onChange = vi.fn();
		render(
			<SearchField
				aria-label="Search"
				clearLabel="Clear"
				value="q"
				onChange={onChange}
			/>,
		);
		await user.click(screen.getByRole("button", { name: "Clear" }));
		expect(onChange).toHaveBeenCalledWith("");
		expect(document.activeElement).toBe(
			screen.getByRole("searchbox", { name: "Search" }),
		);
	});

	it("Esc clears", async () => {
		const user = userEvent.setup();
		const onChange = vi.fn();
		render(
			<SearchField
				aria-label="Search"
				clearLabel="Clear"
				value="q"
				onChange={onChange}
			/>,
		);
		await user.click(screen.getByRole("searchbox"));
		await user.keyboard("{Escape}");
		expect(onChange).toHaveBeenCalledWith("");
	});
});
