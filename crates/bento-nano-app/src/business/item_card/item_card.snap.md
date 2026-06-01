# ItemCard — visual snap

A single file/folder tile inside an `ItemGrid`. Two layout variants:
Standard (vertical: icon-on-top, name-below) and Wide (horizontal,
spans 2 grid columns).

| token              | value                                  |
|--------------------|----------------------------------------|
| min-width          | 88 px (Standard) / 200 px (Wide)       |
| min-height         | 76 px (Standard) / auto (Wide). nano renders each card at `ITEM_GRID_ROW_HEIGHT_PX` = 80 px — a deliberate grid-aligned super-set so the card pixel-aligns with its parent grid row. |
| corner-radius      | 10 px (`--radius-card`)                |
| background         | `--surface-subtle` `rgba(255,255,255,0.03)` |
| missing background | `rgba(239,68,68,0.08)` (softened)      |
| padding            | 8 px vert / 4 px horiz (Standard); 10 px vert / 12 px horiz (Wide) |
| layout             | column / center / center (Standard)    |
|                    | row / start / center (Wide)            |
| icon size          | 28 px container (Wide) / 36 px (Standard)|
| name font          | 11 px default (panel-scope FontGroup)  |
| name color         | `--text-secondary` `#c0c0cc`           |
| name max-lines     | 2 (Standard) / 1 (Wide)                |
| transition         | `all var(--transition-fast)` = 150 ms ease-out (base) |
| hover              | translateY(-1px) + scale(1.02), 150 ms ease-out (`--transition-fast`). FIX 1: `CARD_HOVER_LIFT_DY * hover_t` lift; dropped while actively pressed (CSS `:active` scale-only specificity). |
| hover background   | lerp `--surface-subtle` → `--surface-hover` `rgba(255,255,255,0.08)` by hover_t |
| hover border       | 1 px stroke, alpha transparent → `--border-hover` `rgba(255,255,255,0.2)` by hover_t |
| hover shadow       | `--shadow-item-hover` two-layer: `0 2px 8px rgba(0,0,0,0.12)` (contact) + `0 8px 24px rgba(0,0,0,0.08)` (ambient), alpha × hover_t |
| press              | scale(0.97), 80 ms (overrides `--transition-fast`) |
| focus-visible      | 2 px `--accent-blue` outline @ 2 px offset, border transparent. **DEFERRED in nano** — no per-item keyboard-focus signal exists (`ZoneItem` has no `selected`/`focused` field); paint once that channel lands. |
| selected           | background `--surface-active` + 1 px border `--zone-accent` (fallback `--accent-blue`). **Unwired in nano** — no per-item selection signal exists; same deferral as focus-visible. |
| dragging opacity   | 0.3 + `pointer-events: none` (no explicit transition; rides base 150 ms). nano models this with distinct fill colours (drag-source `surface_alt @0.18`, floating ghost `surface @0.86`) rather than a literal opacity multiplier. |
| reduced-motion     | `transition-duration`/`animation-duration: 0.01ms` (base + `:active`) |
| missing badge      | 9 px pill, `palette.danger` (Wide only)|
| double-click       | open file (delegated to host)          |
| right-click        | open context menu (delegated to host)  |
| keyboard           | tabIndex=0, role=button                |
| missing → disabled | aria-disabled=true; hover lift + bg/border/shadow chrome all removed (FIX 2: nano zeroes `hover_t` for missing cards) |
| span-2 column      | when `is_wide` true                    |

Reference 1.x source: `bentodesk/src/components/ItemCard/ItemCard.tsx` (126 LOC).

Locked behaviour:
- `display_name(name)` strips trailing `.lnk` / `.url` from the rendered
  label (case-insensitive). The on-disk name is never mutated.
- `CardVariant` is the wire-format-locked enum; defaults to `Standard`.
- `column_span()` mirrors `item_grid::column_span_for(is_wide)` so the
  card's grid hint and the grid layout always agree.
