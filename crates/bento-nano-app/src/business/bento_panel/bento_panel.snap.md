# BentoPanel — visual snap

Expanded-state container that hosts a focused BentoZone. Wraps a column of:
PanelHeader (icon · title · search · close) → optional SearchBar → ItemGrid.

| token            | value                                  |
|------------------|----------------------------------------|
| width            | `Length::Auto` (zone-bounded)          |
| height           | `Length::Auto` (zone-bounded)          |
| padding          | top 12 px, sides 16 px, bottom 16 px   |
| corner-radius    | 14 px                                  |
| background       | `palette.surface_primary` (frosted)    |
| border           | 1 px `palette.outline_subtle`          |
| layout           | column / start / stretch               |
| header-height    | 48 px (Tauri `.panel-header`, M2③ 1:1) |
| header-padding    | 0 / 16 px (Tauri `padding: 0 --spacing-lg`)        |
| header-gap        | 8 px between flex children (`--spacing-sm`)        |
| header-children   | icon · title (flex:1) · badge · actions[search,close] |
| header-top-accent| 2 px solid `var(--zone-accent, transparent)` (on `.bento-zone--expanded`, NOT the header) |
| header-divider   | 1 px `rgba(255,255,255,0.05)` border-bottom, spans FULL header width (no L/R inset) |
| header-icon       | 18 px glyph, NO background chip, flex-shrink:0 (`IconKind`) |
| header-title-font | 14 px / weight 500 / color text-primary / nowrap / flex:1; Tauri `letter-spacing: 0.3px` APPROXIMATED (omitted — see deviations) |
| header-badge      | bg `var(--zone-accent, --badge-bg)` · radius `--radius-badge` · padding 2px 9px · 11 px / weight 600 · text-primary · line-height 1.4 · INTRINSIC width · placed BEFORE the actions group |
| header-btn        | 28×28 · radius 6 px · color text-muted · 14×14 glyph (search=magnifier `IconKind::Search`, close=X `IconKind::X`) |
| header-btn-hover  | search: bg `--surface-hover`+text-primary; close: bg rgba(239,68,68,0.2)+accent-red — DEFERRED in nano (see deviations) |
| header-gap-below | 8 px                                   |
| search-row-h     | 32 px (when expanded)                  |
| search-collapsed | 0 px (animated 200 ms ease-out)        |
| item-card font   | 14 px (Tauri `--font-size-md`; live `draw_item_card` SSoT, item_card.snap.md:18) |
| item-grid cols   | 4 (`PANEL_DEFAULT_GRID_COLUMNS`)       |
| escape           | closes panel (handled by host)         |

Reference 1.x source: `bentodesk/src/components/BentoZone/BentoPanel.tsx`
(77 LOC) + `PanelHeader.tsx` (96 LOC) + `PanelHeader.css` (83 LOC).

Locked behaviour:
- The panel-scope **font group** is what gives an item-grid column its
  uniform glyph size (1.x v8 `FontGroupContext`). The item-card label font
  size is 14px (Tauri `--font-size-md`), sourced from the single live literal
  at the `draw_item_card` label draw — the stale `PANEL_ITEM_CARD_FONT_PX =
  11.0` scaffold constant was removed (#1 step 14, 2026-06-02).
- Default grid column count is exposed as `PANEL_DEFAULT_GRID_COLUMNS = 4`.
- The header / search / grid are three direct children laid out vertically
  with fixed gaps, no virtualization at the panel level (virtualization
  belongs to `ItemGrid`).
- **PanelHeader actions** (GROUP-4 2026-06-01 1:1): the search button opens
  search for the zone (Tauri `openSearch(zone.id)`); the close button
  collapses the expanded panel back to its pill (Tauri `onClose()`). Geometry
  for the icon / badge / both buttons is the paint==hit SSoT in
  `expanded_zone_grid::ExpandedZoneLayout`.

nano deviations from Tauri (intentional, documented):
- **Title `letter-spacing: 0.3px`** is NOT applied — nano's DWrite text path
  has no per-run character-spacing seam wired at this call site, and at 14 px
  the 0.3 px tracking is sub-pixel. Approximated without it.
- **Header-button hover** (`--surface-hover` / close-red) is DEFERRED — there
  is no per-button hover signal for the panel header yet, so only the base
  (transparent) button state is painted; the glyphs render at `text_muted`.
  When a header-button hover channel lands, lerp the fill + glyph colour.
