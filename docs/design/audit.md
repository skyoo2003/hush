# UI audit — 2026-11-26 (D1)

The baseline for the design sessions. Every screen and state below was rendered by the UI harness (`VITE_MOCK_IPC=1 pnpm dev`, then open `/__harness`) and photographed with `pnpm ui:snap` in WebKit at 2x. No UI was changed in this session.

Screenshot paths follow `ui-snapshots/{screen}/{state}-{theme}-{lang}.png`, with theme `light|dark` and lang `en|ko`. The harness renders main at the DESIGN.md size, 1080x720. The real main window opens at 960x680 today (`tauri.conf.json`), and quick search opens at 680x460.

## Summary

The app has no visual design yet. Apart from two theme rules and one inline outline, every pixel is the WebKit user-agent stylesheet. Text is 16px Times. Headings are 24/32px serif. Controls are 11px mini system buttons. Both windows read as an unstyled web page. Dark mode does nothing, because no `color-scheme` is declared. All 82 light/dark pairs are byte-identical (measured), and `prefers-color-scheme: dark` is true inside the frame. The main window is one long scrolling column of seven settings sections. It has no sidebar, toolbar, Air-Gap indicator, or search entry. The product's core screens (cited Q&A, compare, the source viewer) do not exist in the frontend yet.

## Screen inventory

| Window | Screen / region | State | Screenshot (`ui-snapshots/…`) |
|---|---|---|---|
| quick-search (680x460, no decorations) | Search panel | empty (hint) | `quick-search/empty-*` |
| | | loading (query sent, no answer) | `quick-search/loading-*` |
| | | results, first hit selected | `quick-search/results-*` |
| | | results, Korean + English, long file name | `quick-search/results-mixed-*` |
| | | keyboard selection on the 3rd hit | `quick-search/keyboard-nav-*` |
| | | filter chips (`type:` `after:`) | `quick-search/filters-*` |
| | | partial: still indexing 342 files | `quick-search/partial-indexing-*` |
| | | no results | `quick-search/no-results-*` |
| | | model missing | `quick-search/model-missing-*` |
| | | search error | `quick-search/error-*` |
| | | open failed: source moved | `quick-search/source-missing-*` |
| | | citation copied (⌘C) | `quick-search/copied-*` |
| main (one scrolling page) | Folders | overview: active, paused, unavailable folders, errors | `main/overview-*` |
| | | first run: no folders, never indexed | `main/first-run-*` |
| | | indexing in progress | `main/indexing-*` |
| | | indexing paused | `main/indexing-paused-*` |
| | | failed files expanded | `main/folder-errors-*` |
| | | remove confirmation (inline fieldset) | `main/folder-remove-confirm-*` |
| | | broad-path warning after Add | `main/folder-broad-path-*` |
| | | add failed (overlap) | `main/folder-add-error-*` |
| | Exclude patterns, Schedule | allowed hours + low power | `main/schedule-*` |
| | Quick search shortcut | shortcut held by another app | `main/shortcut-conflict-*` |
| | Security: Air-Gap, Models, App updates | Air-Gap on | `main/airgap-on-*` |
| | | Air-Gap off | `main/airgap-off-*` |
| | | first run: model setup | `main/first-run-model-*` |
| | | model downloading (39%) | `main/model-downloading-*` |
| | | update available | `main/update-available-*` |
| | | update check failed | `main/update-error-*` |
| | Appearance, Data and diagnostics | default | `main/theme-and-data-*` |
| | | delete-all confirmation | `main/delete-confirm-*` |
| | | export failed | `main/export-failed-*` |
| | Sponsor | default | `main/sponsor-*` |
| | | QR (Air-Gap on) | `main/sponsor-qr-*` |
| | | supporter badge | `main/supporter-*` |
| | Milestone banner (sticky bottom) | shown | `main/sponsor-banner-*` |
| | About | default | `main/about-*` |
| | | license text expanded | `main/about-license-open-*` |
| | Whole page | OLED dark theme | `main/theme-oled-dark-*` |
| | | Terminal theme | `main/theme-terminal-*` |
| | | loading (no command answered) | `main/loading-*` |
| | | every command failed | `main/backend-error-*` |

Not in the harness, because nothing in the frontend renders them:

- Cited Q&A answer, "Not found in your local documents", compare with a conflict, the source viewer, and the evidence-level and AI-summary labels. No IPC command or generated type exists for them (`src-tauri/src/commands.rs`), so the mock has nothing to type them against. The fixtures were skipped, not invented.
- The tray menu and window chrome. They are native, not web.

## Findings

Severity: P0 blocks use, P1 broken or confusing, P2 polish. "Rule" quotes `docs/design/DESIGN.md`.

| # | Screen | Problem | Sev | Rule it breaks | Suggested direction |
|---|---|---|---|---|---|
| 1 | All | Dark mode is ignored. No `color-scheme` is declared, so a dark-mode Mac gets a white window. All 82 light/dark pairs are identical (measured). | P0 | Color: support light and dark | Declare `color-scheme: light dark` and build on semantic tokens (D2) |
| 2 | All | Body text is the 16px Times default (`-webkit-standard`). Headings are 24/32px serif, and Korean falls back to a serif (Myungjo) face. | P1 | Fonts: system fonts; Type scale: Body 13 | System font stack and the macOS type scale as tokens (D2) |
| 3 | All | No tokens. 7 hard-coded color values, 0 radii, 0 shadows, and 1 inline style. Everything else is the UA stylesheet. | P1 | Never hard-code colors…; use tokens | `tokens.css` from the style inventory below (D2) |
| 4 | Quick search | The panel is a bare page. The search field is a 156px mini field with 11px text (measured), not a full-width input. Results scroll the whole document, so the field scrolls away. No 12px radius, no vibrancy. | P1 | Materials: HUD/Popover vibrancy; radius 12 for quick search | Fixed search header, scrolling results list, panel material (D4) |
| 5 | Quick search | Arrow-key selection leaves the viewport. After 7× ↓ the selected hit is at top 705px in a 460px window with scrollTop 0 (measured). | P1 | Keyboard: the selection must stay visible | `scrollIntoView({ block: "nearest" })` on selection change (D4) |
| 6 | Quick search | The selected hit is shown only by a 2px `Highlight` outline at 60% alpha: 1.49:1 against white (measured), below the 3:1 non-text minimum. | P1 | Selection uses the accent background with white text | Accent-filled row when key, gray when not (D3/D4) |
| 7 | Quick search | Confirmation and errors sit outside the view. "Citation copied." renders after the whole list, below the fold in `copied-light-en`. The key legend also scrolls off. | P1 | States: feedback the user can see | Status line in a fixed footer with the key hints (D4) |
| 8 | Quick search | Errors look like body text, with no icon, color, or recovery action. "Search is off until the embedding model is installed" gives no way to the install. | P1 | error (with a recovery action); model missing | Shared state component with an action button (D6) |
| 9 | Quick search | Loading shows nothing: the field and an empty panel (`loading-*`). | P2 | loading: skeleton after 150ms | Delayed skeleton rows (D6) |
| 10 | Quick search | File icons are color emoji (📕📘📄🖼️). | P2 | Icons: Lucide 16/14px | Lucide file-type icons (D3) |
| 11 | Quick search | Filter chips are a bulleted list. The source location (`p. 4`, section path) is plain text next to the file name, not a source chip. | P2 | Source chip `[n] file · p.X`, monospaced number | Chip component (D3), used in result rows (D4) |
| 12 | Main | The window is one long document: seven `<h2>` sections stacked in a 1080px column with a 32px app-name `<h1>`. No sidebar, no toolbar, and no room left for the traffic lights. | P0 | Window chrome: Overlay title bar, 78px inset; Sidebar 220px | Sidebar + content shell with Settings panes (D5) |
| 13 | Main | No Air-Gap indicator in a toolbar. Air-Gap is an unstyled checkbox in the middle of the page, and "Off" looks the same as "On": no shield, no green/amber. | P0 | Trust UI: Air-Gap indicator in the toolbar, green on / amber off | Toolbar indicator + Settings > Privacy switch (D5) |
| 14 | Main | The main window has no search. Searching is only possible from the global shortcut. | P1 | Content is the hero | Search field in the main toolbar or a content area (D5) |
| 15 | Main | Controls are 11px "mini" buttons, 18px tall (measured), next to 16px text, so every control is smaller than the words around it. | P1 | Controls: regular 22–28px | Button and field primitives at regular size (D3) |
| 16 | Main | Folder rows are bullet-list items that mix a checkbox, a bold label, a 13.3px path, counts, and two buttons in one line. Paths are not monospaced, and the long rows wrap in Terminal and Korean. | P1 | List rows 44–56px with a secondary line; mono for paths | Folder list rows with a secondary line (D7) |
| 17 | Main | Errors and notices render at the bottom of the Folders section, far from the control that caused them. `folder-add-error` shows the overlap message under the low-power picker, below the fold of the Add button. | P1 | error with recovery, near its cause | Inline message under the control (D6/D7) |
| 18 | Main | Destructive actions look like every other button. "Delete everything listed and restart" and "Remove" are gray mini buttons, and the delete confirmation is inline page text. | P1 | Native feel; dangerous actions stand out | Destructive button style + a sheet/alert dialog (D3/D7) |
| 19 | Main | Fieldsets use the UA groove border spanning 990px, with radios run together ("Automatic○ Paused○"). | P2 | Spacing on a 4px grid | Segmented control or radio group with spacing (D7) |
| 20 | Main | The model setup dumps raw URLs and 64-char SHA-256 strings into the first-run flow. | P2 | Content first, chrome calm | Collapse the file list behind a disclosure, mono small text (D7) |
| 21 | Main | The QR code is an unframed 200px image floating under the sponsor buttons. | P2 | Cards radius 10 | Framed card (D7) |
| 22 | Main | OLED dark and Terminal themes change the page colors but not the controls: buttons, textarea, and select stay light gray, and checkboxes keep the default blue. The quick-search window never applies `data-theme`, so it stays in the system look. | P1 | Themes selectable by everyone, consistent | Themes as token overrides, applied in both windows (D2) |
| 23 | Main | Loading and backend-error states show partial UI. Empty textarea, a "…" shortcut button, a disabled but checked Air-Gap box, and six separate "Could not read…" lines. | P2 | loading and error states | Section-level skeleton and one error state with retry (D6) |
| 24 | Main | The sponsor goal (`FUNDING_GOAL`) is English inside the Korean UI: "현재 목표: Stage 1, the Apple Developer Program annual fee…". | P2 | All UI strings go through i18n | Localize the goal text in `funding.rs`/i18n (D8) |
| 25 | All | Keyboard focus is the WebKit default ring: thin, pale, not accent. With macOS default keyboard settings, Tab skips buttons and checkboxes and reaches only fields and selects (measured: 3 Tabs went straight to the Theme select). | P1 | Focus: 3px accent ring on `:focus-visible` | Focus-ring token on every control (D3) |
| 26 | All | Disabled buttons ("Save patterns", "Download", "Check for updates") render text at 26% alpha: about 1.86:1 (inferred from the computed color). Disabled controls are exempt from WCAG 1.4.3, but these sit next to the reason they are off. | P2 | Text contrast | Readable disabled style; keep the reason text at full contrast (D2/D3) |
| 27 | All | Default text cursor and selectable text everywhere. Labels, headings, and buttons can be selected, and result rows use the auto cursor. | P2 | Selection off on chrome, on in content | `user-select: none` on chrome, `text` on snippets (D3) |
| 28 | Main | The real window opens at 960x680 (`tauri.conf.json`), not the 1080x720 default, and has no minimum size. | P2 | Main window 1080x720, minimum 760x520 | Window config in the shell session (D5) |
| 29 | Korean | Korean strings fit everywhere checked; wrapping happens only in long body paragraphs. In the Terminal theme the Receipts folder row wraps its "Remove" button onto its own line (`theme-terminal-light-en`). | P2 | Layouts survive Korean and 1.5x strings | Re-check after typography changes (D8) |

Contrast that passes (measured): body black on white 21:1, highlight mark black on yellow 19.6:1, OLED text 17.1:1, Terminal green on its background 14.5:1. The color failures are the selection outline (#6) and the disabled-text case (#26).

## Top 10, in priority order

| Rank | Finding | Why first | Fixed in |
|---|---|---|---|
| 1 | #1 Dark mode ignored | Every dark-mode Mac gets a white window | D2 |
| 2 | #12 Main window is a web page with no shell | Everything else hangs off the shell | D5 |
| 3 | #13 No Air-Gap indicator | The product's trust promise is invisible | D5 |
| 4 | #2 / #3 16px serif type, no tokens | Every later session needs tokens | D2 |
| 5 | #4 Quick-search panel is a bare page | The most-used surface | D4 |
| 6 | #5 Keyboard selection leaves the view | Arrow keys are the main way to use quick search | D4 |
| 7 | #6 Selection nearly invisible (1.49:1) | You cannot tell what Enter will open | D3/D4 |
| 8 | #8 / #17 Errors look like text, far from their cause, no recovery | Users miss failures | D6 |
| 9 | #15 Mini 11px controls | Hard to hit and read; most visibly "not a Mac app" | D3 |
| 10 | #25 Focus ring | Keyboard users lose their place | D3 |

D-session numbers are a proposal; the repository has no design-session plan yet (only D2 and D9 are named in the D1 brief). D2 tokens, type, dark mode, and themes. D3 primitives in `src/ui/` (buttons, fields, rows, chips, focus, icons). D4 quick-search panel. D5 main-window shell (sidebar, toolbar, Air-Gap indicator, window config). D6 shared states (loading, empty, error, not found, blocked). D7 settings panes. D8 Korean, Increase Contrast, reduced motion pass. D9 committed screenshot baseline.

## Style inventory (input for D2)

Where styles live today:

| Place | What | Count |
|---|---|---|
| `src/theme.css` (imported by `src/main.tsx`) | Two themes on `:root[data-theme]`, `.sponsor-banner`, `.legal-text` | 1 file, 5 rules |
| Inline `style=` | `SearchResultCard.tsx:42`, `outline: 2px solid Highlight` on the selected hit | 1 |
| `className` | `quick-search` (no rule matches it), `sponsor-banner`, `legal-text` | 3 |
| Libraries | None. No CSS framework, no icon set, no CSS-in-JS. Dependencies are `@tauri-apps/api`, `react`, `react-dom`. | 0 |
| HTML attributes | `width={200} height={200}` on the sponsor QR image | 2 |
| UA stylesheet | Everything else: fonts, sizes, margins, controls, focus | — |

Distinct values (grep over `src/theme.css`, `src/components`, `src/App.tsx`, `src/main.tsx`, `index.html`; `src/dev` excluded):

| Kind | Value | Uses | Where |
|---|---|---|---|
| Color | `#000` | 1 | OLED background |
| Color | `#e8e8e8` | 1 | OLED text |
| Color | `#0a0f0a` | 1 | Terminal background |
| Color | `#39ff7a` | 1 | Terminal text |
| Color | `Canvas` (system color) | 1 | Banner background |
| Color | `currentColor` | 1 | Banner top border |
| Color | `Highlight` (system color) | 1 | Selected search hit outline |
| Color scheme | `color-scheme: dark` | 2 | OLED, Terminal |
| Font family | `ui-monospace, Menlo, monospace` | 1 | Terminal theme |
| Font size | none set | 0 | UA defaults: 16px body, 24px h2, 18.72px h3, 32px h1, 13.33px `small`, 13px `code`, 11px controls (measured) |
| Font weight | none set | 0 | `<strong>` and headings only |
| Radius | none set | 0 | — |
| Shadow | none set | 0 | — |
| Border | `1px solid currentColor` | 1 | Banner |
| Outline | `2px solid Highlight` | 1 | Selected hit |
| Spacing | `padding: 0.5em 1em` | 1 | Banner |
| Size | `max-height: 24rem` | 1 | Legal text |
| Motion | none | 0 | No transitions or animations |
| Media queries | none | 0 | No `prefers-color-scheme`, `prefers-contrast`, or `prefers-reduced-motion` |

Rendered sizes on `main/overview` (measured from computed styles): h1 32px ×1, h2 24px ×7, h3 18.72px ×3, body `p`/`li`/`label`/`legend`/`summary` 16px, `small` 13.33px ×4, `code` 13px monospace ×13, buttons/select/textarea 11px system-ui ×21.

## Reproduce

```sh
VITE_MOCK_IPC=1 pnpm dev   # http://localhost:1420/__harness
pnpm ui:snap               # all 164 screenshots, ~3 min
pnpm ui:snap main/delete   # only names starting with main/delete
```
