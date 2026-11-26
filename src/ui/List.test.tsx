// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { List, ListRow } from "./List";

const NAMES = ["Alpha", "Banana", "Bravo", "Bob", "Charlie"];

function Harness({
	initial = null,
	onChange = () => {},
	onOpen = () => {},
}: {
	initial?: string | null;
	onChange?: (key: string) => void;
	onOpen?: (key: string) => void;
}) {
	const [selected, setSelected] = useState<string | null>(initial);
	return (
		<List
			label="Fruits"
			items={NAMES}
			itemKey={(n) => n.toLowerCase()}
			itemText={(n) => n}
			selectedKey={selected}
			onSelectedKeyChange={(k) => {
				onChange(k);
				setSelected(k);
			}}
			onOpen={onOpen}
			density="dense"
		>
			{(n) => <ListRow title={n} />}
		</List>
	);
}

const option = (name: string) => screen.getByRole("option", { name });
const selectedName = () =>
	screen.getByRole("option", { selected: true }).textContent;
const tabbables = () =>
	screen.getAllByRole("option").filter((o) => o.tabIndex === 0);

const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));

afterEach(cleanup);

describe("List", () => {
	it("makes only the first option tabbable when nothing is selected", () => {
		render(<Harness />);
		expect(screen.getByRole("listbox", { name: "Fruits" })).toBeTruthy();
		expect(tabbables()).toEqual([option("Alpha")]);
		expect(screen.queryByRole("option", { selected: true })).toBeNull();
	});

	it("moves selection, focus and the tab stop with the arrows, without wrapping", async () => {
		const user = userEvent.setup();
		const onChange = vi.fn();
		render(<Harness initial="alpha" onChange={onChange} />);
		await user.tab();
		expect(document.activeElement).toBe(option("Alpha"));

		await user.keyboard("{ArrowDown}");
		expect(onChange).toHaveBeenLastCalledWith("banana");
		expect(option("Banana").getAttribute("aria-selected")).toBe("true");
		expect(option("Alpha").getAttribute("aria-selected")).toBe("false");
		expect(document.activeElement).toBe(option("Banana"));
		expect(tabbables()).toEqual([option("Banana")]);

		await user.keyboard("{ArrowUp}{ArrowUp}");
		expect(selectedName()).toBe("Alpha");
		expect(document.activeElement).toBe(option("Alpha"));

		await user.keyboard("{End}{ArrowDown}");
		expect(selectedName()).toBe("Charlie");
		expect(tabbables()).toEqual([option("Charlie")]);

		await user.keyboard("{Home}");
		expect(selectedName()).toBe("Alpha");
		expect(document.activeElement).toBe(option("Alpha"));
	});

	it("opens the selected item with Enter and on double-click", async () => {
		const user = userEvent.setup();
		const onOpen = vi.fn();
		render(<Harness initial="bravo" onOpen={onOpen} />);
		await user.tab();
		await user.keyboard("{Enter}");
		expect(onOpen).toHaveBeenLastCalledWith("bravo");
		await user.dblClick(option("Bob"));
		expect(selectedName()).toBe("Bob");
		expect(onOpen).toHaveBeenLastCalledWith("bob");
	});

	it("selects by typing: one letter, a multi-letter prefix, cycling, and reset after 500ms", async () => {
		const user = userEvent.setup();
		render(<Harness initial="alpha" />);
		await user.tab();

		await user.keyboard("b");
		expect(selectedName()).toBe("Banana");
		await user.keyboard("r");
		expect(selectedName()).toBe("Bravo");
		expect(document.activeElement).toBe(option("Bravo"));

		await pause(600);
		await user.keyboard("b");
		expect(selectedName()).toBe("Bob");
		await user.keyboard("b");
		expect(selectedName()).toBe("Banana");
		await user.keyboard("b");
		expect(selectedName()).toBe("Bravo");

		// Without the reset the buffer "bbba" would match nothing; after it, "a" finds Alpha.
		await pause(600);
		await user.keyboard("a");
		expect(selectedName()).toBe("Alpha");
	});
});
