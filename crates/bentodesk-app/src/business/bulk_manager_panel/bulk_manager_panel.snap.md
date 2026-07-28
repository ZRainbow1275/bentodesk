# BulkManagerPanel — Visual Fidelity Snapshot

Per Ruling R3. 1.x source: `bentodesk/src/components/BulkManager/BulkManagerPanel.tsx`
+ `BulkManagerPanel.css`. Modal that lists every zone in a sortable table,
lets the user multi-select (per-row checkbox), and runs bulk actions
(Hide / Show / Delete / Move / Auto Layout) on the selection in a single
transaction.

## Geometry

- Modal panel: **min(960 px, 92vw) × min(640 px, 80vh)**, `palette.surface`
  background, 1 px `palette.border` outline, 16 px corner radius.
- Outer panel padding: **20 px** uniform.
- Header row: **52 px tall** — title (`palette.text`, 18 pt Semibold) on the
  left, search input (240 px wide × 32 px tall, `palette.surface_alt` bg) in
  the middle, close `IconButton` (32 × 32 px, 8 px corner radius) on the right.
- Toolbar row: **44 px tall** — Select-all toggle, Invert button, divider,
  Hide / Show / Delete primary buttons, then a counter (`{selected}/{total}`)
  pinned right. The selected-stack runtime D2D panel renders two compact
  pointer rows for All / Invert / Hide / Show / Grid / Row / Col / Spiral /
  Organic / Update / Delete / Move / Close above the table rows.
- Table area: vertical scroll inside the remaining height; rows alternate
  `palette.surface` / `palette.surface_alt` background; selected row gets a
  2 px `palette.accent` left edge.
- Footer action bar: **56 px tall** — pinned to the panel bottom, contains
  the four primary actions when at least one row is selected; collapses to
  zero height when `selected_count == 0`.

## Table columns

| Column | Width | Content |
|--------|-------|---------|
| Checkbox | 36 px | `Checkbox` widget; column header is the select-all toggle. |
| Name | flex 2 | Zone alias or fallback `name`, `palette.text` 14 pt. |
| State | 72 px | `visible` / `hidden`; hidden rows remain selectable so Show can restore them. |
| Items | 80 px | Item count, right-aligned. |
| Accent | 96 px | Swatch (16 px circle, 1 px `palette.border`) + hex label. |
| Size | 80 px | `{w}x{h}%` rounded, right-aligned. |
| Position | 80 px | `{x},{y}` rounded, right-aligned. |

Row height: **44 px**. Cell vertical padding: 8 px.

## Selection contract

- Per-row checkbox toggles `Vec<ZoneId>` membership (the canonical truth
  is the panel's `selected: SmallVec<[ZoneId; 8]>` — small inline buffer
  matches the Wave-E search-bar pattern).
- The header checkbox flips into "deselect all" when every visible row is
  selected.
- `Invert` flips membership for every row currently visible (post-search).
- Selection survives sort/search re-flow because it's keyed by `ZoneId`,
  not by row index.

## Bulk actions

| Button | When enabled | Action recorded | Drained `Command` |
|--------|--------------|-----------------|--------------------|
| Hide | `selected.len() > 0` | `BulkManagerAction::Hide { ids }` | shell forwards `Command::BulkSetZonesVisible { visible: false }` |
| Show | `selected.len() > 0` | `BulkManagerAction::Show { ids }` | shell forwards `Command::BulkSetZonesVisible { visible: true }` |
| Delete | `selected.len() > 0` | `BulkManagerAction::Delete { ids }` | shell forwards `Command::BulkDeleteZones` |
| Move… | `selected.len() > 0` | `BulkManagerAction::Move { ids, delta }` | shell forwards `Command::BulkMoveZones` |
| Auto Layout | selected rows or listed rows | focused keyboard producer | shell forwards `Command::BulkApplyLayout { ids, algorithm }` |
| Update | selected rows or listed rows | focused keyboard / pointer producer | shell forwards `Command::BulkUpdateZones(Vec<BulkZoneUpdate>)` |
| Text | `selected.len() > 0` | focused keyboard / pointer producer opens typed field editor | Enter forwards `Command::BulkUpdateZones(Vec<BulkZoneUpdate>)` after validation |
| Close (header X) | always | `BulkManagerAction::Close` | shell hides the host window — no Command |

`take_action()` is one-shot per the dialog pattern; the shell drains
once per frame and translates to the appropriate Command sequence.

Focused keyboard layout producers mirror the Tauri auto-layout algorithms:
`G` = Grid, `R` = Row, `C` = Column, `P` = Spiral, and `O` = Organic. If no
row is selected, the shell applies the algorithm to the currently listed rows.
Runtime pointer hit-testing shares the same button and row rectangles with the
renderer. Clicking a visible row toggles that row's stable `ZoneId` selection;
clicking an action button routes through the same producer as the matching
keyboard shortcut. `T` or the visible Text button opens an in-panel typed field
editor for selected rows. While active, `WM_CHAR` appends draft text, Backspace
edits it, `F2` cycles Alias/Icon/Accent/Capsule/Mode, Enter validates and
emits `BulkUpdateZones`, and Esc cancels without mutation.

## Sort + search

- Sort key cycles `Name → Items → Accent → Size` on header click;
  same-key click toggles ascending / descending.
- Search input filters the visible row set (case-insensitive substring
  on `alias ?? name`); search does NOT mutate selection.

## Window class

- Hosted in its own modal HWND (falls back to the
  `WindowKind::Settings`-shape until the platform factory grows a
  `WindowKind::BulkManager` variant — same pattern as
  `smart_group_suggestor`).
- `WS_EX_NOREDIRECTIONBITMAP | WS_POPUPWINDOW | WS_CAPTION | WS_VISIBLE`.

## Hibernation

§11 R5 eligible — close button, scrim click, or Escape triggers
`WM_SHOWWINDOW(false)` and the per-window swap chain release.

## Smoke verification

`mod tests` proves:

- `BulkManagerState::set_zones` seeds the row list and clears selection.
- `toggle_selection` adds/removes ids; `select_all` / `deselect_all` /
  `invert_selection` operate on the visible-after-search set.
- `set_search` filters the visible row set without losing selection.
- `set_sort_key` cycles direction on same-key click.
- `click_hide` / `click_show` / `click_delete` / `click_move` / `click_close`
  record the matching action; only enabled when `can_act()` (selected > 0).
- runtime pointer hit-testing maps visible buttons and visible rows to stable
  actions without relying on log-only paths.
- `take_action` is one-shot.
- `build()` returns the chrome Container at the panel size.
