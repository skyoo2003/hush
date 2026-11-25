# Locite — Design Rules (read before any UI work)

## Character

Locite is a quiet, native-feeling macOS utility. Content (excerpts and their sources) is the hero; chrome stays calm. When unsure, do what a well-made Apple app (Finder, Notes, Spotlight, System Settings) would do. Reference: Apple Human Interface Guidelines for macOS.

## Hard constraints (CLAUDE.md still applies)

- CSP: no external fonts, CDNs, images, or scripts. System fonts only; never ship font files.
- The quick-search window keeps its minimal capabilities. A new window gets its own capabilities file with the least permissions.
- All UI strings go through i18n (en source, ko). Layouts must survive Korean text and English strings up to 1.5x longer than the placeholder.
- No feature locks: every theme is selectable by everyone. settings.supporter only toggles the badge and the banner.
- Sponsorship appears only in Settings > Sponsor, the tray menu, and the one-time milestone banner.
- Design sessions do not change IPC contracts or Rust behavior unless the session says so. If a design needs a new IPC, propose it and stop.

## Platform feel

- Fonts: `-apple-system, BlinkMacSystemFont, system-ui` for text; `ui-monospace` (SF Mono) for paths, numbers in chips, and shortcuts.
- Type scale follows macOS text styles: Large Title 26, Title 1 22, Title 2 17, Title 3 15, Headline 13 semibold, Body 13, Callout 12, Subheadline 11, Footnote 10. Body text is 13px, not 16px.
- Color: semantic tokens only (tokens.css). Support light and dark (`prefers-color-scheme`) and Increase Contrast (`prefers-contrast: more`). Follow the system accent color, pushed from Rust as a CSS variable; Locite's own accent applies only when the user picks it.
- Materials: the sidebar and the quick-search panel use native vibrancy (window-vibrancy crate: Sidebar, HUDWindow/Popover) with a transparent web background behind them. Content areas are opaque.
- Window chrome: titleBarStyle Overlay with a hidden title. Leave room for the traffic lights (about 78px left inset). The toolbar area is draggable (`data-tauri-drag-region`) except for its controls.
- Controls: native sizes (regular 22–28px tall, small 18–20px). Radius 6px for controls, 10px for cards and panels, 12px for the quick-search panel.
- Focus: a 3px accent focus ring on `:focus-visible`, always visible for keyboard users.
- Cursor: the default arrow for buttons and list rows, as on macOS. The hand cursor is only for real links.
- Text selection is off on chrome (buttons, labels, sidebar) and on in content (snippets, answers, viewer).
- Motion: 120–200ms ease-out for show/hide, no bounce, nothing longer than 250ms. With `prefers-reduced-motion`, fade only.
- Icons: Lucide (ISC license), 16px with a 1.5px stroke in chrome, 14px in dense rows. Do not copy SF Symbols artwork.
- No sounds.

## Spacing and layout

- 4px grid: 4, 8, 12, 16, 20, 24, 32.
- Main window default 1080x720, minimum 760x520. Sidebar 220px, resizable 180–300.
- List rows are 28px (dense) or 44–56px with a secondary line. No zebra striping. Selection uses the accent background with white text when the window is key, and gray when it is not.

## Trust UI (identical everywhere)

- Air-Gap indicator: a shield icon + "Air-Gap On/Off" in the toolbar trailing area and in the tray. Calm green when on, amber when off. Clicking opens Settings > Privacy.
- Source chip: `[n] file label · p.X`, monospaced number, accent outline. Hover shows the excerpt; click jumps to the source.
- Evidence level: Strong / Weak / None as icon + text, never color alone.
- "Not found in your local documents" is its own neutral component, never styled as an error.
- AI-generated text always carries a small "AI summary — check the sources" label.

## States every screen must design

loading (skeleton only after 150ms; no spinner for waits under 300ms), empty (first use), no results, partial (indexing in progress), error (with a recovery action), blocked by Air-Gap, model missing.

## Process

- Never hard-code colors, sizes, radii, shadows, or durations in components. Use tokens.
- New UI pieces go into src/ui/ with a story in the UI harness before any screen uses them.
- After each change, run `pnpm ui:snap` and look at the screenshots of the screens you touched (light/dark × en/ko). Record before/after paths in docs/progress.md.
