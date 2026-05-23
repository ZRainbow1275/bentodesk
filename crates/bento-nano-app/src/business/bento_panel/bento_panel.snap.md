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
| header-height    | 36 px (locked)                         |
| header-gap-below | 8 px                                   |
| search-row-h     | 32 px (when expanded)                  |
| search-collapsed | 0 px (animated 200 ms ease-out)        |
| item-card font   | 11 px (`PANEL_ITEM_CARD_FONT_PX`)      |
| item-grid cols   | 4 (`PANEL_DEFAULT_GRID_COLUMNS`)       |
| escape           | closes panel (handled by host)         |

Reference 1.x source: `bentodesk/src/components/BentoZone/BentoPanel.tsx`
(77 LOC) + `PanelHeader.tsx` (96 LOC).

Locked behaviour:
- The panel-scope **font group** is what gives an item-grid column its
  uniform glyph size (1.x v8 `FontGroupContext`). The default font px is
  exposed as `PANEL_ITEM_CARD_FONT_PX = 11.0`.
- Default grid column count is exposed as `PANEL_DEFAULT_GRID_COLUMNS = 4`.
- The header / search / grid are three direct children laid out vertically
  with fixed gaps, no virtualization at the panel level (virtualization
  belongs to `ItemGrid`).
