# Information architecture — D5 shell, menu bar, Settings

Status: accepted and built, 2026-11-26 (ADR-026). The decisions at the end were settled with the recommended options.

Goal: make Locite look and behave like a native macOS app. The main window becomes the library (sidebar, toolbar, content), Settings moves to its own window, and a native menu bar and tray reach every existing feature. The panels keep their current behavior.

## Windows

| Label | Role | Size | Chrome | Capabilities |
|---|---|---|---|---|
| `main` | Library: search, folders, indexing status | default 1080x720, min 760x520 (DESIGN.md) | Overlay title bar, hidden title, traffic lights inset (x 18, y 22), Sidebar vibrancy behind the sidebar only | `main-window.json`, reduced (below) |
| `settings` | Settings, one pane at a time | 680x560, width fixed, height resizable (min 420) | Overlay title bar, hidden title, traffic lights at (14, 14), pane name centered in the title row, tab toolbar under it, opaque | new `settings-window.json` |
| `quick-search` | Unchanged | 680x460 | Unchanged | Unchanged |

- Both `main` and `settings` remember size and position (tauri-plugin-window-state, geometry only). The sidebar width (180–300, default 220) is stored per window in `localStorage` (`sidebar-width:main`), because it is UI geometry, not document data (absolute rule 4 covers paths, snippets, vectors, history).
- Vibrancy uses Tauri's built-in `windowEffects` (`sidebar`, follows window active state) with `transparent: true`. That needs `app.macOSPrivateApi: true`. Locite does not ship on the Mac App Store, so the private-API restriction costs nothing. No new crate: Tauri already wraps window-vibrancy.
- Closing `main` or `settings` hides it, as `main` does today. The Dock icon shows while either window is visible and goes back to Accessory when both are hidden.
- Only one `settings` window exists. ⌘, shows it and brings it to the front. Opening it at a pane (from the Air-Gap indicator, the tray, or a menu item) switches the pane.

### Viewer placement

The PDF viewer opens **inside the main window**. The pack viewer opens in **its own window**.

- PDF viewer in main. A source is opened from a search result, an answer citation, or a compare row, and the user then goes back to that result. The library already has back/forward history, so the viewer is one more route: ⌘[ returns to the result list with the selection and scroll kept. A separate window would lose that context and leave a pile of windows behind each click. Finder's Quick Look and Mail's message view make the same choice. "Open in Default App" stays as an action for anyone who wants Preview.
- Pack viewer in its own window, one per pack. A `.locitepack` is a document someone else made: it arrives from Finder (double-click, `PACK_EXT` file association), it is not part of the user's library, and two packs may be compared side by side. Its own window can carry its own least-privilege capability (read that one pack, no folder or settings commands), and it keeps foreign content visibly apart from the user's own documents. This is how Preview and Pages treat documents.
- Neither viewer exists today (`open_source` opens the file in the default app; `export_pack` returns not implemented). This session builds neither. It reserves the `viewer` route in main and the `pack-<id>` window label.

## Main window

```
┌──────────────────────────────────────────────────────────────────────┐
│ ● ● ●   ‹ ›          [ 🔍 Search your folders          ]  🛡 Air-Gap On  + │  toolbar (drag region)
├───────────────┬──────────────────────────────────────────────────────┤
│ Search     ⌘1 │                                                      │
│ Ask        ⌘2 │   content of the selected route                      │
│ Compare    ⌘3 │                                                      │
│ Changes    ⌘4 │                                                      │
│               │                                                      │
│ FOLDERS       │                                                      │
│ ● Research 412│                                                      │
│ ⏸ Receipts  38│                                                      │
│ ⚠ Archive   — │                                                      │
│               │                                                      │
│ ◔ Indexing    │                                                      │
│   120/480     │                                                      │
└───────────────┴──────────────────────────────────────────────────────┘
```

### Routes

One typed route drives the sidebar selection, the content, back/forward, and the menu context:

```ts
type Route =
  | { kind: "search"; query: string }
  | { kind: "ask" }
  | { kind: "compare" }
  | { kind: "changes" }
  | { kind: "folder"; id: number }
  | { kind: "viewer"; chunkId: number }; // reserved, not built
```

History is an array of routes plus an index. Back/forward (toolbar, ⌘[ / ⌘]) moves the index. Typing in the toolbar field replaces the current search route instead of pushing one per keystroke; pressing Enter or picking a result pushes.

### Sidebar

| Item | Content | Uses |
|---|---|---|
| Search | Toolbar field drives a result list (`List` + `SearchResultCard` on D3 parts), filter chips, the same states as quick search (empty, loading, no results, partial, model missing, error). Return opens, ⌘C on a selected row with no text selection copies the citation. | `search`, `open_source`, `copy_citation` |
| Ask | See open decision 1 | — |
| Compare | See open decision 1 | — |
| Changes | See open decision 1 | — |
| Folders › each folder | Header: label, mono path, status, counts (documents, failed, skipped). Failed files with Retry (moved from the panel's expansion). A "Search this folder" field that runs search with a `folder:` chip. Context menu: Pause/Resume Watching, Rescan, Show in Settings. | `list_folders`, `set_folder_enabled`, `list_failed_documents`, `retry_document`, `search` |

Folder rows show a status icon (active, paused, unavailable, error) as icon and text in the tooltip, never color alone, and the document count as a trailing mono count. An unavailable folder still lists, with a warning icon.

Sidebar footer: a `ProgressRing` and "Indexing 120/480", "Paused 120/480", or "Up to date" (the same rules as the tray's `index_label`). Clicking it opens a `Popover` with the progress numbers, the schedule state and reason, and a "Folders & Indexing Settings…" button. Pause/Resume stays in the Indexing menu and the tray, so main does not need `set_schedule`.

### Toolbar

Leading after the traffic lights: back, forward (`IconButton`, disabled at the ends of history). Center: `SearchField`, ⌘F focuses it and selects its text, and typing from anywhere outside a text field does not steal focus. Trailing: `AirGapIndicator` (click opens Settings › Privacy & Air-Gap), then "+" (Add Folder…, same flow as today's Add button, including the broad-path warning and overlap error, shown as an `Alert` sheet).

The milestone `SponsorBanner` stays in main, docked at the bottom of the content area.

## Settings window

Tabs in the toolbar, icon over label, in this order. Each pane reuses today's panel logic, rebuilt on D3 components.

| Tab | Content (from) |
|---|---|
| General | Software Update: check, install (`SecurityPanel` update part). Data: export diagnostics, delete all indexes with an `Alert` (`DataPanel`). |
| Folders & Indexing | Folder table with add, remove (`Alert`), enable; exclude patterns; schedule: mode, allowed hours, low power (`FolderAndSchedulePanel`). |
| Models | Model list, download with progress, cancel, import from a kit (`SecurityPanel` model part). |
| Privacy & Air-Gap | Air-Gap switch with what it blocks, link to "How to verify Air-Gap" (`SecurityPanel` Air-Gap part). |
| Shortcuts | Quick-search shortcut recorder and conflict state (`ShortcutSetting`), then a read-only list of the app's shortcuts (the same table as Help › Keyboard Shortcuts). |
| Appearance | Theme, accent (`ThemeSetting`). |
| Sponsor | Channels, QR when Air-Gap is on, supporter badge switch (`SponsorPanel`). |
| About | Version, source link, license and notices (`AboutPanel`). |

General carries Software Update and Data because neither has another home in the list, and both are what macOS apps put under General. The window opens on the last pane used. Every pane stays mounted and only the current one is shown, so events (a tray QR, model download progress) reach their pane and a pane keeps its state across tab switches. Air-Gap state is owned by the window and shared by Privacy, Models, and General.

### Capability split

`settings-window.json` (window `settings`): `list_folders`, `add_watched_folder`, `remove_watched_folder`, `set_folder_enabled`, `list_failed_documents`, `retry_document`, `set_exclude_patterns`, `get_index_status`, `get_schedule`, `set_schedule`, `get_settings`, `set_air_gap`, `set_shortcut`, `set_supporter`, `set_theme`, `set_accent`, `get_appearance`, `open_sponsor`, `list_models`, `download_model`, `cancel_model_download`, `import_model_file`, `check_for_update`, `install_update`, `export_diagnostics`, `delete_all_indexes`, `get_about`, `read_legal_text`, event listen/unlisten.

`main-window.json` keeps: `search`, `open_source`, `copy_citation`, `list_folders`, `add_watched_folder`, `set_folder_enabled`, `list_failed_documents`, `retry_document`, `get_index_status`, `get_schedule` (footer popover), `get_settings`, `get_appearance`, `dismiss_sponsor_banner`, `export_pack`, `open_settings` (new), `set_menu_context` (new), event listen/unlisten.

Main loses: `remove_watched_folder`, `set_exclude_patterns`, `set_schedule`, `set_air_gap`, `set_shortcut`, `set_supporter`, `set_theme`, `set_accent`, `open_sponsor`, `list_models`, `download_model`, `cancel_model_download`, `import_model_file`, `check_for_update`, `install_update`, `export_diagnostics`, `delete_all_indexes`, `get_about`, `read_legal_text`. A compromised main webview can no longer turn Air-Gap off or delete the index.

`main` gains `search`, `open_source`, `copy_citation` (they were quick-search only). That is the cost of search in the library (audit #14).

## Menu bar

Built in Rust with the Tauri Menu API, rebuilt when the language or the shortcut changes. Labels come from a Rust string table (en, ko) keyed like `i18n.ts`; the language is the first macOS preferred language that Locite supports, the same one WKWebView reports to `navigator.language`. The tray labels move to the same table, so they are localized too.

| Menu | Items | Enabled when |
|---|---|---|
| Locite | About Locite (Settings › About), Settings… ⌘,, Sponsor Locite… (Settings › Sponsor), —, Services, —, Hide Locite ⌘H, Hide Others ⌥⌘H, Show All, —, Quit Locite ⌘Q | always |
| File | Add Folder… ⌘O | always (shows main first) |
| | Open Quick Search (accelerator shows the current global shortcut) | the shortcut is registered |
| | Export as .locitepack… ⇧⌘E | see open decision 1 |
| | Close Window ⌘W | a Locite window is key |
| Edit | Undo ⌘Z, Redo ⇧⌘Z, —, Cut ⌘X, Copy ⌘C, Paste ⌘V, Select All ⌘A (predefined items, so WKWebView text fields get them) | always (the responder chain decides) |
| | Copy Citation | main is key and a result is selected |
| | Copy Masked ⇧⌘C | see open decision 1 |
| | Find ⌘F | main is key |
| View | Show/Hide Sidebar ⌃⌘S (title flips) | main is key |
| | Search ⌘1, Ask ⌘2, Compare ⌘3, Changes ⌘4 (checkmark on the current one) | main is key |
| | Enter Full Screen ⌃⌘F (predefined) | a resizable window is key |
| Indexing | Pause Indexing / Resume Indexing | indexing started (same rule as the tray) |
| | Index Everything Now | indexing started, not paused, not already run-now (sets `Mode::RunNow`) |
| | Rescan Folder | main is key on a folder route (that folder) |
| Window | predefined Minimize ⌘M, Zoom, —, Bring All to Front; macOS lists the open windows | |
| Help | Locite Help (opens `REPO_URL` docs in the browser), Keyboard Shortcuts ⌘/ (a sheet in main), How to Verify Air-Gap (opens `docs/airgap-verification.md` on GitHub in the browser) | always |

Opening a URL uses `/usr/bin/open` like the sponsor links: the browser makes the request, Locite makes none (absolute rule 2).

Menu items that act inside the web page are emitted to the key window as `menu://action` with a typed `MenuAction`. The web page reports what it can do with `set_menu_context`, and Rust enables or disables items from that plus its own state (key window, schedule, shortcut registration).

### New IPC (needed; DESIGN.md says to propose before building)

```rust
// commands.rs
#[derive(Serialize, Deserialize, TS)] #[serde(rename_all = "snake_case")]
pub enum SettingsPane { General, Folders, Models, Privacy, Shortcuts, Appearance, Sponsor, About }

#[tauri::command] pub fn open_settings<R: Runtime>(app: AppHandle<R>, pane: SettingsPane);

#[derive(Deserialize, TS)] #[serde(rename_all = "camelCase")]
pub struct MenuContext { pub section: Section, pub folder_id: Option<i64>, pub has_selected_hit: bool }
#[derive(Serialize, Deserialize, TS)] #[serde(rename_all = "snake_case")]
pub enum Section { Search, Ask, Compare, Changes, Folder }

#[tauri::command] pub fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: MenuContext);

// events, Rust -> web page
// "menu://action" to the key window
#[derive(Serialize, TS)] #[serde(tag = "kind", rename_all = "snake_case")]
pub enum MenuAction { Navigate { section: Section }, ToggleSidebar, Find, AddFolder, CopyCitation, ShowShortcuts }
// "settings://air-gap" to every window, payload bool, whenever Air-Gap changes
// "settings://pane" to the settings window, payload SettingsPane
```

Neither command takes a path or returns document data. `set_menu_context` takes a folder id, never a path (absolute rule 5 holds if it is logged).

## Tray

Same items as today, as native menu items, with localized labels. The icon becomes a template image (black + alpha, macOS tints it for light, dark, and the highlighted state), 18x18 pt at 1x and 2x, drawn by a script from Lucide shapes so it can be regenerated:

| State (first match wins) | Icon |
|---|---|
| Air-Gap off | shield with a slash |
| Paused | shield with a pause mark |
| Indexing | shield with a small progress arc |
| Idle | plain shield |

Air-Gap off wins because it is the one state the user must not miss. The tooltip carries the same words as the state ("Locite — Air-Gap Off"). The tray "Settings…" item opens the Settings window; a sponsor channel with Air-Gap on opens Settings › Sponsor with the QR.

## Shortcuts

| Keys | Action | Where |
|---|---|---|
| ⌘, | Settings | menu |
| ⌘O | Add Folder | menu |
| ⇧⌘E | Export .locitepack | menu |
| ⌘W | Close Window | menu |
| ⌘F | Focus the toolbar search field | menu |
| ⌘1–⌘4 | Search, Ask, Compare, Changes | menu |
| ⌃⌘S | Show/Hide Sidebar | menu |
| ⌃⌘F | Full Screen | menu |
| ⌘[ / ⌘] | Back / Forward | web page (main) |
| ⌘C on a selected result, no text selected | Copy Citation | web page (`copy` event), as in quick search |
| ⇧⌘C | Copy Masked | menu |
| ⌘/ | Keyboard Shortcuts sheet | menu |
| global (default ⌥Space) | Quick Search | global shortcut |

## Harness

Scenarios move from the old long page to routes and panes. Every route and every pane gets at least a default state; the existing 29 main states map onto the pane that now holds them (for example `main/airgap-off` becomes `settings/privacy-airgap-off`). Each renders light/dark × en/ko. The harness draws a fake title bar and traffic-light spacer, because WebKit has no vibrancy or native chrome. The native menu, the tray, and vibrancy are checked in the running app with verify-locite.

## Decisions (settled 2026-11-26: option A, the plugin, the Rust table)

1. **Ask, Compare, Changes, Export .locitepack, Copy Masked have no backend.** No IPC exists for cited Q&A, compare, or change history (roadmap weeks 9–11), `export_pack` returns not implemented, and `pii_guard::before_copy` is a no-op. Options:
   - A (recommended). Build the routes, sidebar items, and menu items now. Ask, Compare, and Changes open a neutral `EmptyState` ("Arrives in a later version of Locite", no date, no call to action). Export and Copy Masked are present and disabled. Adding the real screens later changes one route component each, and no IPC is invented.
   - B. Leave Ask, Compare, Changes, Export, and Copy Masked out until their week. The sidebar and View menu show Search and Folders only; ⌘2–⌘4 come later. Nothing on screen promises unbuilt work.
   - C. Build mock-backed screens in the harness only, behind no real IPC. More design signal now, but the fixtures would guess at contracts that do not exist (D1 refused that).
2. **Window state crate.** tauri-plugin-window-state (MIT/Apache-2.0, no network) writes window geometry to a JSON file in the app config folder. The alternative is storing geometry in the SQLCipher settings table by hand (about 60 lines of Rust and two more events). Recommended: the plugin, with an ADR, since geometry is not covered by absolute rule 4.
3. **Rust-side strings.** Recommended: a Rust table (`desktop/strings.rs`) for menu and tray, where each key's `match` must cover both languages, so a missing translation does not compile. The alternative, one JSON shared with `i18n.ts`, keeps one file but couples the native layer to the web build.
