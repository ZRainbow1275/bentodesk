# SelectionFloatingBar — visual snap

Floating action bar that appears above the bounding box of the current
multi-item selection. Auto-shown when `SelectionState::count() >= 2`,
auto-hidden when count returns to 0 or 1.

- **Surface:** `theme::current().palette.surface_elevated`,
  `BorderRadius::all(10.0)`, drop shadow.
- **Padding:** `Edges { left: 8, right: 8, top: 6, bottom: 6 }`.
- **Layout:** `Direction::Row`, `gap = 4.0`.
- **Anchor:** centred horizontally on the selection bbox, vertically
  positioned `ANCHOR_OFFSET = 12 px` above the bbox top.
- **Flip:** when there's no room above (top < ANCHOR_OFFSET + bar height
  + viewport edge margin), flip to `ANCHOR_OFFSET` below the bbox bottom.

## Action buttons (left → right)

1. **Count chip** — `format!("{n} selected")` in a 24 px tall pill.
2. **Move** — folder→folder icon, opens move-to-zone popover.
3. **Tag** — tag icon, opens tag picker popover.
4. **Group** — boxes icon, dispatches `Command::GroupSelection`.
5. **Delete** — trash icon, dispatches `Command::DeleteSelection` (with
   confirm dialog gate inside the dispatcher).
6. **Dismiss** — X icon, clears selection.

Each button is 28×28 px, hover bg `palette.surface_hover`.

## Wire constants

- `BAR_HEIGHT = 40.0`, `ANCHOR_OFFSET = 12.0`.
- `MIN_SELECTION_COUNT: usize = 2` — appearance threshold.
- `BUTTON_SIZE = 28.0`, `BUTTON_GAP = 4.0`.

## Hibernation contract

Bar is part of the main window — no separate swap chain. Visibility is
purely state-driven (selection count); shell does not hibernate it
independently.
