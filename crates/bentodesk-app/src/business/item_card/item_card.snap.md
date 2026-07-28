# ItemCard — visual snap

A single file/folder tile inside an `ItemGrid`. Two layout variants:
Standard (vertical: icon-on-top, name-below) and Wide (horizontal,
spans 2 grid columns).

| token              | value                                  |
|--------------------|----------------------------------------|
| min-width          | 88 px (Standard) / 200 px (Wide)       |
| min-height         | 76 px (Standard) / auto (Wide). native renders each card at `ITEM_GRID_ROW_HEIGHT_PX` = 78 px (Tauri row height; stride 78 + 8 gap = 86) so the card pixel-aligns with its parent grid row. |
| corner-radius      | 10 px (`--radius-card`)                |
| background         | `--surface-subtle` `rgba(255,255,255,0.03)` |
| missing background | `rgba(239,68,68,0.08)` (softened)      |
| padding            | 8 px vert / 4 px horiz (Standard); 10 px vert / 12 px horiz (Wide) |
| layout             | column / center / center (Standard)    |
|                    | row / start / center (Wide)            |
| icon size          | 36 px container / 24 px render (Standard); 28 px container / 20 px render (Wide) |
| name font          | 2026-06-02 reference-frame effective 14 px / weight 400, single line. The later source CSS nominal token is 11 px (`--font-size-xs`), but the video-parity continuation treats captured runtime frames as authoritative when source and frame evidence conflict. |
| name color         | 2026-06-02 reference-frame primary ink (`--text-primary` / `#f0f0f5` in dark); the later source CSS says `--text-secondary`, but the video-parity continuation treats captured runtime frames as authoritative when source and frame evidence conflict. |
| name max-lines     | 2 (Standard) / 1 (Wide)                |
| transition         | `all var(--transition-fast)` = 150 ms ease-out (base) |
| enter animation    | `itemEnter` 250 ms ease-out, opacity 0→1 + translateY(6 px)→0, staggered by `index * 30 ms` with no extra base delay; parent bento/content layers own their own visibility delays. native applies this during the pill→expanded morph and caps later slots to finish inside the 500 ms morph envelope. |
| hover              | translateY(-1px) + scale(1.02), 150 ms ease-out (`--transition-fast`). FIX 1: `CARD_HOVER_LIFT_DY * hover_t` lift; dropped while actively pressed (CSS `:active` scale-only specificity). |
| hover background   | lerp `--surface-subtle` → `--surface-hover` `rgba(255,255,255,0.08)` by hover_t |
| hover border       | 1 px stroke, alpha transparent → `--border-hover` `rgba(255,255,255,0.2)` by hover_t |
| hover shadow       | `--shadow-item-hover` two-layer: `0 2px 8px rgba(0,0,0,0.12)` (contact) + `0 8px 24px rgba(0,0,0,0.08)` (ambient), alpha × hover_t |
| press              | scale(0.97), 80 ms (overrides `--transition-fast`) |
| focus-visible      | 2 px `--accent-blue` outline @ 2 px offset, border transparent. **DEFERRED in native** — no per-item keyboard-focus signal exists (`ZoneItem` has no `selected`/`focused` field); paint once that channel lands. |
| selected           | background `--surface-active` + 1 px border `--zone-accent` (fallback `--accent-blue`). **Unwired in native** — no per-item selection signal exists; same deferral as focus-visible. |
| dragging opacity   | 0.3 + `pointer-events: none` (no explicit transition; rides base 150 ms). native models this with distinct fill colours (drag-source `surface_alt @0.18`, floating ghost `surface @0.86`) rather than a literal opacity multiplier. |
| reduced-motion     | `transition-duration`/`animation-duration: 0.01ms` (base + `:active`) |
| missing badge      | 9 px pill, `palette.danger` (Wide only)|
| double-click       | open file (delegated to host)          |
| right-click        | open context menu (delegated to host)  |
| keyboard           | tabIndex=0, role=button                |
| missing → disabled | aria-disabled=true; hover lift + bg/border/shadow chrome all removed (FIX 2: native zeroes `hover_t` for missing cards) |
| span-2 column      | when `is_wide` true                    |

Reference 1.x source: `bentodesk/src/components/ItemCard/ItemCard.tsx` (126 LOC).

Locked behaviour:
- `display_name(name)` strips trailing `.lnk` / `.url` from the rendered
  label (case-insensitive). The on-disk name is never mutated.
- `CardVariant` is the wire-format-locked enum; defaults to `Standard`.
- `column_span()` mirrors `item_grid::column_span_for(is_wide)` so the
  card's grid hint and the grid layout always agree.
