// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Table, type TableSort } from "./Table";

type Row = { name: string; size: number };
const ROWS: Row[] = [
	{ name: "a", size: 1 },
	{ name: "b", size: 2 },
];

afterEach(cleanup);

function setup(sort: TableSort | null) {
	const onSortChange = vi.fn();
	render(
		<Table
			label="Folders"
			rows={ROWS}
			rowKey={(r) => r.name}
			columns={[
				{ key: "name", header: "Name", sortable: true, render: (r) => r.name },
				{ key: "size", header: "Size", align: "end", render: (r) => r.size },
			]}
			sort={sort}
			onSortChange={onSortChange}
		/>,
	);
	return onSortChange;
}

const header = (name: string) => screen.getByRole("columnheader", { name });

describe("Table", () => {
	it("renders rows and leaves unsorted columns without aria-sort", () => {
		setup(null);
		expect(screen.getByRole("table", { name: "Folders" })).toBeTruthy();
		expect(screen.getAllByRole("row")).toHaveLength(3);
		expect(header("Name").hasAttribute("aria-sort")).toBe(false);
		expect(screen.getAllByRole("button")).toHaveLength(1);
	});

	it("sorts ascending first, then toggles the direction", async () => {
		const user = userEvent.setup();
		const onSortChange = setup(null);
		await user.click(screen.getByRole("button", { name: "Name" }));
		expect(onSortChange).toHaveBeenLastCalledWith({
			key: "name",
			direction: "ascending",
		});
		cleanup();

		const again = setup({ key: "name", direction: "ascending" });
		expect(header("Name").getAttribute("aria-sort")).toBe("ascending");
		await user.click(screen.getByRole("button", { name: "Name" }));
		expect(again).toHaveBeenLastCalledWith({
			key: "name",
			direction: "descending",
		});
		cleanup();

		setup({ key: "name", direction: "descending" });
		expect(header("Name").getAttribute("aria-sort")).toBe("descending");
	});
});
