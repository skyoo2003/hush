// SPDX-License-Identifier: GPL-3.0-or-later
// Shortcuts use global-hotkey syntax ("shift+alt+KeyK"), which Rust parses and stores.

type KeyLike = Pick<
  KeyboardEvent,
  "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey"
>;

const MODIFIER_CODE = /^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/;

/** The shortcut a keydown would set, or null while only modifiers (or only Shift) are held. */
export function shortcutFromKey(e: KeyLike): string | null {
  if (MODIFIER_CODE.test(e.code) || !(e.altKey || e.ctrlKey || e.metaKey))
    return null;
  const mods = [
    e.shiftKey && "shift",
    e.ctrlKey && "control",
    e.altKey && "alt",
    e.metaKey && "super",
  ].filter(Boolean);
  return [...mods, e.code].join("+");
}

const SYMBOL: Record<string, [number, string]> = {
  control: [0, "⌃"],
  ctrl: [0, "⌃"],
  alt: [1, "⌥"],
  option: [1, "⌥"],
  shift: [2, "⇧"],
  super: [3, "⌘"],
  cmd: [3, "⌘"],
  command: [3, "⌘"],
};

/** "shift+alt+KeyK" -> "⌥⇧K", in macOS modifier order. */
export function formatShortcut(shortcut: string): string {
  const parts = shortcut.split("+");
  const key = (parts.pop() ?? "").replace(/^(Key|Digit)/, "");
  const mods = parts
    .map((m): [number, string] => SYMBOL[m.toLowerCase()] ?? [4, m])
    .sort((a, b) => a[0] - b[0])
    .map(([, s]) => s);
  return mods.join("") + key;
}
