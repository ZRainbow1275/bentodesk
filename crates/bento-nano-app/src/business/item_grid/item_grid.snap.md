# ItemGrid — visual snap

CSS-Grid-equivalent layout that arranges ItemCards inside a BentoPanel.
For zones with item count `< VIRTUAL_THRESHOLD` (50) renders every card;
above that, hands off to `VirtualItemGrid` (TBD widget-library work).

| token             | value                                    |
|-------------------|------------------------------------------|
| layout            | grid, `grid_columns` cols (default 4)    |
| row-height        | 80 px (`ITEM_GRID_ROW_HEIGHT_PX`)        |
| column-gap        | 8 px (`ITEM_GRID_COLUMN_GAP_PX`)         |
| row-gap           | 8 px (`ITEM_GRID_ROW_GAP_PX`)            |
| virtual-threshold | 50 items (`ITEM_GRID_VIRTUAL_THRESHOLD`) |
| virt-overscan     | 3 rows (`ITEM_GRID_OVERSCAN_ROWS`)       |
| wide-card span    | 2 columns (`is_wide` items)              |
| ghost-card        | inserted at drop index during DnD        |
| empty-state       | none (parent handles "no items" copy)    |
| selection         | per-item highlight (handled by ItemCard) |

Reference 1.x sources:
- `bentodesk/src/components/BentoZone/ItemGrid.tsx` (106 LOC)
- `bentodesk/src/components/BentoZone/VirtualItemGrid.tsx` (118 LOC)

Locked behaviour:
- `pick_layout(item_count, grid_columns)` returns one of three modes:
  `Direct` (count < 50), `Virtual` (count ≥ 50), or `Empty` (count == 0).
  This decision is **final** before the children are built — no per-frame
  re-checks.
- Wide items consume two grid cells; the layout helper exposes a
  `column_span_for(is_wide)` constant so the composition layer doesn't
  branch on every card.
