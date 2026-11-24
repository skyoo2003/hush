// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { formatShortcut, shortcutFromKey } from "./shortcut";

const key = (
  code: string,
  mods: Partial<Record<"alt" | "ctrl" | "meta" | "shift", boolean>> = {},
) => ({
  code,
  altKey: !!mods.alt,
  ctrlKey: !!mods.ctrl,
  metaKey: !!mods.meta,
  shiftKey: !!mods.shift,
});

describe("shortcutFromKey", () => {
  it("builds global-hotkey syntax in its modifier order", () => {
    expect(shortcutFromKey(key("Space", { alt: true }))).toBe("alt+Space");
    expect(
      shortcutFromKey(key("KeyK", { meta: true, shift: true, ctrl: true })),
    ).toBe("shift+control+super+KeyK");
  });

  it("waits while only modifiers are down", () => {
    expect(shortcutFromKey(key("AltLeft", { alt: true }))).toBeNull();
    expect(shortcutFromKey(key("MetaRight", { meta: true }))).toBeNull();
  });

  it("rejects a key with no modifier or only Shift, which would swallow typing", () => {
    expect(shortcutFromKey(key("KeyK"))).toBeNull();
    expect(shortcutFromKey(key("KeyK", { shift: true }))).toBeNull();
  });
});

describe("formatShortcut", () => {
  it("shows macOS symbols in macOS order", () => {
    expect(formatShortcut("alt+Space")).toBe("⌥Space");
    expect(formatShortcut("shift+control+super+KeyK")).toBe("⌃⇧⌘K");
    expect(formatShortcut("Alt+Shift+Digit1")).toBe("⌥⇧1");
  });
});
