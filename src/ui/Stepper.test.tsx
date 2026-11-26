// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Stepper } from "./Stepper";

function Harness({ initial }: { initial: number }) {
	const [value, setValue] = useState(initial);
	return (
		<Stepper
			label="Results"
			incrementLabel="Increase"
			decrementLabel="Decrease"
			value={value}
			onValueChange={setValue}
			min={1}
			max={3}
		/>
	);
}

afterEach(cleanup);

describe("Stepper", () => {
	it("exposes spinbutton state and steps with arrow keys, clamped", async () => {
		const user = userEvent.setup();
		render(<Harness initial={2} />);
		const field = screen.getByRole("spinbutton", { name: "Results" });
		expect(field.getAttribute("aria-valuemin")).toBe("1");
		expect(field.getAttribute("aria-valuemax")).toBe("3");
		expect(field.getAttribute("aria-valuenow")).toBe("2");

		await user.click(field);
		await user.keyboard("{ArrowUp}{ArrowUp}");
		expect(field.getAttribute("aria-valuenow")).toBe("3");
		await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
		expect(field.getAttribute("aria-valuenow")).toBe("1");
	});

	it("buttons step and disable at the bounds; typed values clamp", async () => {
		const user = userEvent.setup();
		render(<Harness initial={2} />);
		const field = screen.getByRole("spinbutton");
		await user.click(screen.getByRole("button", { name: "Increase" }));
		expect(field.getAttribute("aria-valuenow")).toBe("3");
		expect(screen.getByRole("button", { name: "Increase" })).toHaveProperty(
			"disabled",
			true,
		);

		await user.clear(field);
		await user.type(field, "-5{Enter}");
		expect(field.getAttribute("aria-valuenow")).toBe("1");
		expect((field as HTMLInputElement).value).toBe("1");
	});
});
