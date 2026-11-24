// SPDX-License-Identifier: GPL-3.0-or-later

export const DEBOUNCE_MS = 200;

export type Segment = { text: string; match: boolean };

/** Cuts `snippet` at `highlights` (UTF-16 `[start, end)`, the unit JS strings index by). */
export function splitHighlights(
  snippet: string,
  highlights: Array<[number, number]>,
): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  for (const [start, end] of [...highlights].sort((a, b) => a[0] - b[0])) {
    if (start < at || end <= start || end > snippet.length) continue;
    if (start > at) out.push({ text: snippet.slice(at, start), match: false });
    out.push({ text: snippet.slice(start, end), match: true });
    at = end;
  }
  if (at < snippet.length) out.push({ text: snippet.slice(at), match: false });
  return out;
}

export type SearchUpdate<R> =
  | { kind: "idle" }
  | { kind: "done"; result: R }
  | { kind: "error"; error: unknown };

/**
 * Debounces queries and tags each with an id; a response is applied only while its id is the
 * newest, so a slow earlier search never replaces a later one.
 */
export function createSearcher<R>(
  run: (query: string) => Promise<R>,
  apply: (update: SearchUpdate<R>) => void,
  delayMs = DEBOUNCE_MS,
) {
  let latest = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    set(query: string) {
      const id = ++latest;
      clearTimeout(timer);
      if (!query.trim()) {
        apply({ kind: "idle" });
        return;
      }
      timer = setTimeout(() => {
        run(query).then(
          (result) => id === latest && apply({ kind: "done", result }),
          (error) => id === latest && apply({ kind: "error", error }),
        );
      }, delayMs);
    },
    /** Drops the pending query and any response still in flight. */
    cancel() {
      latest++;
      clearTimeout(timer);
    },
  };
}

export type KeyCommand = "up" | "down" | "open" | "reveal" | "copy" | "close";

type Key = { key: string; metaKey: boolean; isComposing?: boolean };

/** `textSelected`: the input has selected text, so Cmd+C copies that text instead. */
export function keyCommand(e: Key, textSelected: boolean): KeyCommand | null {
  // Enter and arrows during IME composition (Korean, Japanese) belong to the IME.
  if (e.isComposing) return null;
  switch (e.key) {
    case "ArrowUp":
      return "up";
    case "ArrowDown":
      return "down";
    case "Enter":
      return e.metaKey ? "reveal" : "open";
    case "Escape":
      return "close";
    case "c":
    case "C":
      return e.metaKey && !textSelected ? "copy" : null;
    default:
      return null;
  }
}

/** The selection after `cmd`, kept inside `[0, count)`. */
export function moveSelection(
  selected: number,
  cmd: KeyCommand,
  count: number,
): number {
  const step = cmd === "down" ? 1 : cmd === "up" ? -1 : 0;
  return Math.max(0, Math.min(selected + step, count - 1));
}
