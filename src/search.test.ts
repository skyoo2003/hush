// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createSearcher,
  type KeyCommand,
  keyCommand,
  moveSelection,
  type SearchUpdate,
  splitHighlights,
} from "./search";

describe("splitHighlights", () => {
  it("marks matches in order, whatever order they arrive in", () => {
    expect(
      splitHighlights("a cat sat", [
        [6, 9],
        [2, 5],
      ]),
    ).toEqual([
      { text: "a ", match: false },
      { text: "cat", match: true },
      { text: " ", match: false },
      { text: "sat", match: true },
    ]);
  });

  it("indexes by UTF-16 units, like the backend", () => {
    // "😀" is two UTF-16 units, so "문서" starts at 3.
    expect(splitHighlights("😀 문서", [[3, 5]])).toEqual([
      { text: "😀 ", match: false },
      { text: "문서", match: true },
    ]);
  });

  it("drops overlapping and out-of-range spans", () => {
    expect(
      splitHighlights("abcdef", [
        [0, 3],
        [2, 4],
        [5, 99],
      ]),
    ).toEqual([
      { text: "abc", match: true },
      { text: "def", match: false },
    ]);
  });
});

describe("keyboard", () => {
  const key = (k: string, metaKey = false, isComposing = false) => ({
    key: k,
    metaKey,
    isComposing,
  });

  it("maps keys to commands", () => {
    expect(keyCommand(key("ArrowDown"), false)).toBe("down");
    expect(keyCommand(key("ArrowUp"), false)).toBe("up");
    expect(keyCommand(key("Enter"), false)).toBe("open");
    expect(keyCommand(key("Enter", true), false)).toBe("reveal");
    expect(keyCommand(key("c", true), false)).toBe("copy");
    expect(keyCommand(key("Escape"), false)).toBe("close");
    expect(keyCommand(key("c"), false)).toBeNull();
  });

  it("leaves Cmd+C to the input when it has selected text", () => {
    expect(keyCommand(key("c", true), true)).toBeNull();
  });

  it("leaves Enter and arrows to an IME that is composing", () => {
    expect(keyCommand(key("Enter", false, true), false)).toBeNull();
    expect(keyCommand(key("ArrowDown", false, true), false)).toBeNull();
  });

  it("moves through results and stops at both ends", () => {
    const walk = (cmds: KeyCommand[], count: number) =>
      cmds.reduce((i, cmd) => moveSelection(i, cmd, count), 0);
    expect(walk(["down", "down"], 3)).toBe(2);
    expect(walk(["down", "down", "down", "down"], 3)).toBe(2);
    expect(walk(["down", "up", "up"], 3)).toBe(0);
    expect(walk(["down"], 0)).toBe(0);
  });
});

describe("createSearcher", () => {
  afterEach(() => vi.useRealTimers());

  function setup() {
    vi.useFakeTimers();
    const calls: Array<{
      query: string;
      resolve: (r: string) => void;
      reject: (e: unknown) => void;
    }> = [];
    const updates: Array<SearchUpdate<string>> = [];
    const searcher = createSearcher<string>(
      (query) =>
        new Promise((resolve, reject) =>
          calls.push({ query, resolve, reject }),
        ),
      (u) => updates.push(u),
    );
    return { calls, updates, searcher };
  }

  it("waits 200 ms after the last keystroke and sends one request", () => {
    const { calls, searcher } = setup();
    searcher.set("l");
    vi.advanceTimersByTime(150);
    searcher.set("la");
    vi.advanceTimersByTime(199);
    expect(calls).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(calls.map((c) => c.query)).toEqual(["la"]);
  });

  it("drops a response that arrives after a newer one", async () => {
    const { calls, updates, searcher } = setup();
    searcher.set("old");
    vi.advanceTimersByTime(200);
    searcher.set("new");
    vi.advanceTimersByTime(200);
    calls[1]?.resolve("new result");
    calls[0]?.resolve("old result");
    calls[0]?.reject("ignored");
    await vi.runAllTimersAsync();
    expect(updates).toEqual([{ kind: "done", result: "new result" }]);
  });

  it("drops an in-flight response once the input is cleared", async () => {
    const { calls, updates, searcher } = setup();
    searcher.set("fee");
    vi.advanceTimersByTime(200);
    searcher.set("  ");
    calls[0]?.reject({ code: "search_failed" });
    await vi.runAllTimersAsync();
    expect(updates).toEqual([{ kind: "idle" }]);
  });

  it("drops everything after cancel", async () => {
    const { calls, updates, searcher } = setup();
    searcher.set("a");
    vi.advanceTimersByTime(200);
    searcher.set("b");
    searcher.cancel();
    calls[0]?.resolve("a result");
    await vi.runAllTimersAsync();
    expect(calls).toHaveLength(1);
    expect(updates).toEqual([]);
  });
});
