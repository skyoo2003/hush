// SPDX-License-Identifier: GPL-3.0-or-later
// Sets `data-window="active|inactive"` on <html> as the window gains and loses key status, so
// components can gray their selection and accent fills the way macOS does in a background window.
// CSS reads it as `[data-window="inactive"] .thing`, which also lets the harness force it on a subtree.
export function trackWindowFocus(): void {
	const root = document.documentElement;
	const set = () => {
		root.dataset.window = document.hasFocus() ? "active" : "inactive";
	};
	set();
	window.addEventListener("focus", set);
	window.addEventListener("blur", set);
}
