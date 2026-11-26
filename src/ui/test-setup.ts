// SPDX-License-Identifier: GPL-3.0-or-later
// Shared setup for component tests: happy-dom gaps that Radix touches, and Testing Library cleanup.
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Vitest runs without globals, so Testing Library cannot register its own cleanup.
afterEach(cleanup);

globalThis.ResizeObserver ??= class {
	observe() {}
	unobserve() {}
	disconnect() {}
};
const proto = globalThis.Element?.prototype;
if (proto) {
	proto.hasPointerCapture ??= () => false;
	proto.releasePointerCapture ??= () => {};
	proto.scrollIntoView ??= () => {};
}
