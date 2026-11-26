// SPDX-License-Identifier: GPL-3.0-or-later
// The main window's navigation: one route drives the sidebar selection, the content, back and
// forward, and the menu context sent to Rust.
import type { Section } from "../ipc/Section";

export type Route =
	| { kind: "search" }
	| { kind: "ask" }
	| { kind: "compare" }
	| { kind: "changes" }
	| { kind: "folder"; id: number };

export type History = { stack: readonly Route[]; index: number };

export const START: History = { stack: [{ kind: "search" }], index: 0 };

export const current = (h: History): Route => h.stack[h.index] ?? { kind: "search" };

const same = (a: Route, b: Route) =>
	a.kind === b.kind && (a.kind !== "folder" || (b.kind === "folder" && a.id === b.id));

/** Drops the forward entries, like a browser. Going to the current route changes nothing. */
export function push(h: History, r: Route): History {
	if (same(current(h), r)) return h;
	const stack = [...h.stack.slice(0, h.index + 1), r];
	return { stack, index: stack.length - 1 };
}

export function go(h: History, delta: -1 | 1): History {
	const index = h.index + delta;
	return index < 0 || index >= h.stack.length ? h : { ...h, index };
}

/** Removes a folder that no longer exists, so back never lands on it. */
export function forgetFolder(h: History, id: number): History {
	const keep = (r: Route) => r.kind !== "folder" || r.id !== id;
	const before = h.stack.slice(0, h.index + 1).filter(keep).length;
	const stack = h.stack.filter(keep);
	return stack.length === 0 ? START : { stack, index: Math.max(0, before - 1) };
}

export const section = (r: Route): Section => (r.kind === "folder" ? "folder" : r.kind);

export const routeKey = (r: Route) => (r.kind === "folder" ? `folder:${r.id}` : r.kind);

export function routeFromKey(key: string): Route | null {
	if (key.startsWith("folder:")) return { kind: "folder", id: Number(key.slice(7)) };
	return key === "search" || key === "ask" || key === "compare" || key === "changes"
		? { kind: key }
		: null;
}
