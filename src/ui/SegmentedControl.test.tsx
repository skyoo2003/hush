// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

const options = [
	{ value: "list", label: "List" },
	{ value: "grid", label: "Grid" },
	{ value: "columns", label: "Columns" },
] as const;
type View = (typeof options)[number]["value"];

function Harness({
	onValueChange,
	disabled,
}: {
	onValueChange: (v: View) => void;
	disabled?: boolean;
}) {
	const [value, setValue] = useState<View>("list");
	return (
		<SegmentedControl
			label="View"
			options={options}
			value={value}
			disabled={disabled}
			onValueChange={(v) => {
				setValue(v);
				onValueChange(v);
			}}
		/>
	);
}

afterEach(cleanup);

type User = ReturnType<typeof userEvent.setup>;
// Radix selects on the focus that follows an arrow keydown (on a timer), so release the key after it.
async function press(user: User, key: string) {
	await user.keyboard(`{${key}>}`);
	await user.keyboard(`{/${key}}`);
}

describe("SegmentedControl", () => {
	it("is a named radio group with the value checked", () => {
		render(<Harness onValueChange={() => {}} />);
		expect(screen.getByRole("radiogroup", { name: "View" })).toBeTruthy();
		expect(
			screen.getByRole("radio", { name: "List" }).getAttribute("aria-checked"),
		).toBe("true");
	});

	it("arrow keys move focus and select, wrapping at both ends", async () => {
		const user = userEvent.setup();
		const onValueChange = vi.fn();
		render(<Harness onValueChange={onValueChange} />);
		await user.tab();
		expect(document.activeElement).toBe(
			screen.getByRole("radio", { name: "List" }),
		);

		await press(user, "ArrowRight");
		expect(document.activeElement).toBe(
			screen.getByRole("radio", { name: "Grid" }),
		);
		expect(onValueChange).toHaveBeenLastCalledWith("grid");

		await press(user, "ArrowLeft");
		expect(onValueChange).toHaveBeenLastCalledWith("list");

		await press(user, "ArrowLeft");
		const columns = screen.getByRole("radio", { name: "Columns" });
		expect(document.activeElement).toBe(columns);
		expect(onValueChange).toHaveBeenLastCalledWith("columns");
		expect(columns.getAttribute("aria-checked")).toBe("true");

		await press(user, "ArrowRight");
		expect(document.activeElement).toBe(
			screen.getByRole("radio", { name: "List" }),
		);
		expect(onValueChange).toHaveBeenLastCalledWith("list");
	});

	it("ignores clicks and keys when disabled", async () => {
		const user = userEvent.setup();
		const onValueChange = vi.fn();
		render(<Harness onValueChange={onValueChange} disabled />);
		await user.tab();
		expect(document.activeElement).toBe(document.body);
		await user.click(screen.getByRole("radio", { name: "Grid" }));
		await press(user, "ArrowRight");
		expect(onValueChange).not.toHaveBeenCalled();
	});
});
