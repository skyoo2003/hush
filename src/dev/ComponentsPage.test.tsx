// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { render } from "@testing-library/react";
import axe from "axe-core";
import { expect, it } from "vitest";
import { ComponentsPage } from "./ComponentsPage";

it.each([false, true])(
	"has no axe violations (overlays: %s)",
	async (overlays) => {
		const { container } = render(<ComponentsPage overlays={overlays} />);
		const result = await axe.run(container, {
			// happy-dom has no layout or computed colors; `pnpm ui:axe` checks contrast in WebKit.
			rules: { "color-contrast": { enabled: false } },
		});
		expect(result.passes.length).toBeGreaterThan(10);
		expect(
			result.violations.map(
				(v) => `${v.id}: ${v.nodes.map((n) => n.target).join(" ")}`,
			),
		).toEqual([]);
	},
);
