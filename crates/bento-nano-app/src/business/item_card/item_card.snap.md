# ItemCard — visual snap

A single file/folder tile inside an `ItemGrid`. Two layout variants:
Standard (vertical: icon-on-top, name-below) and Wide (horizontal,
spans 2 grid columns).

| token              | value                                  |
|--------------------|----------------------------------------|
| min-width          | 88 px (Standard) / 200 px (Wide)       |
| height             | 80 px (matches `ITEM_GRID_ROW_HEIGHT_PX`)|
| corner-radius      | 8 px                                   |
| padding            | 6 px (Standard) / 8 px horiz (Wide)    |
| layout             | column / center / center (Standard)    |
|                    | row / start / center (Wide)            |
| icon size          | 28 px container (Wide) / 36 px (Standard)|
| name font          | 11 px default (panel-scope FontGroup)  |
| name max-lines     | 2 (Standard) / 1 (Wide)                |
| selected outline   | 1.5 px `palette.accent_primary`        |
| dragging opacity   | 0.5 (animated 120 ms)                  |
| missing badge      | 9 px pill, `palette.danger` (Wide only)|
| hover lift         | translateY(-1px), 100 ms ease-out      |
| double-click       | open file (delegated to host)          |
| right-click        | open context menu (delegated to host)  |
| keyboard           | tabIndex=0, role=button                |
| missing → disabled | aria-disabled=true, hover lift removed |
| span-2 column      | when `is_wide` true                    |

Reference 1.x source: `bentodesk/src/components/ItemCard/ItemCard.tsx` (126 LOC).

Locked behaviour:
- `display_name(name)` strips trailing `.lnk` / `.url` from the rendered
  label (case-insensitive). The on-disk name is never mutated.
- `CardVariant` is the wire-format-locked enum; defaults to `Standard`.
- `column_span()` mirrors `item_grid::column_span_for(is_wide)` so the
  card's grid hint and the grid layout always agree.
