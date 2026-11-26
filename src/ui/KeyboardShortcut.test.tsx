// @vitest-environment happy-dom
// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { KeyboardShortcut, shortcutGlyphs } from "./KeyboardShortcut";

afterEach(cleanup);

describe("KeyboardShortcut", () => {
	it("orders modifiers ⌃⌥⇧⌘ whatever the prop order, then the uppercased key", () => {
		render(
			<KeyboardShortcut
				keys={{ meta: true, shift: true, alt: true, ctrl: true, key: "k" }}
				label="Control Option Shift Command K"
			/>,
		);
		const kbd = screen.getByRole("img", {
			name: "Control Option Shift Command K",
		});
		expect(kbd.textContent).toBe("⌃⌥⇧⌘K");
	});

	it("maps named keys to glyphs", () => {
		expect(shortcutGlyphs({ meta: true, shift: true, key: "k" })).toBe("⇧⌘K");
		expect(shortcutGlyphs({ key: "Enter" })).toBe("↩");
		expect(shortcutGlyphs({ ctrl: true, key: " " })).toBe("⌃␣");
		expect(shortcutGlyphs({ key: "ArrowLeft" })).toBe("←");
	});
});
