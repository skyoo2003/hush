// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SplitView } from "./SplitView";

afterEach(cleanup);

function setup(size: number) {
	const onSizeChange = vi.fn();
	render(
		<SplitView
			size={size}
			onSizeChange={onSizeChange}
			min={180}
			max={300}
			dividerLabel="Resize sidebar"
			leading="Sidebar"
			trailing="Content"
		/>,
	);
	return {
		onSizeChange,
		divider: screen.getByRole("separator", { name: "Resize sidebar" }),
	};
}

describe("SplitView", () => {
	it("exposes the leading width as the separator value", () => {
		const { divider } = setup(220);
		expect(divider.getAttribute("aria-orientation")).toBe("vertical");
		expect(divider.getAttribute("aria-valuenow")).toBe("220");
		expect(divider.getAttribute("aria-valuemin")).toBe("180");
		expect(divider.getAttribute("aria-valuemax")).toBe("300");
	});

	it("resizes by 8px with the arrow keys", async () => {
		const user = userEvent.setup();
		const { onSizeChange, divider } = setup(220);
		divider.focus();
		await user.keyboard("{ArrowRight}");
		expect(onSizeChange).toHaveBeenLastCalledWith(228);
		await user.keyboard("{ArrowLeft}");
		expect(onSizeChange).toHaveBeenLastCalledWith(212);
	});

	it("clamps to min and max", async () => {
		const user = userEvent.setup();
		const low = setup(184);
		low.divider.focus();
		await user.keyboard("{ArrowLeft}");
		expect(low.onSizeChange).toHaveBeenLastCalledWith(180);
		cleanup();
		const high = setup(296);
		high.divider.focus();
		await user.keyboard("{ArrowRight}");
		expect(high.onSizeChange).toHaveBeenLastCalledWith(300);
	});
});
