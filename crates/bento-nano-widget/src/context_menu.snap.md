# ContextMenu — visual spec (snap.md)

- **Width**: 200 px default; 4 px vertical outer padding.
- **Item row**: 28 px tall (`ITEM_HEIGHT_PX`), `palette.text` color, hover background = `palette.hover_overlay`.
- **Disabled item**: `palette.text_muted` color, no hover background.
- **Divider**: 1 px line of `palette.border`, sandwiched in 4 px vertical padding (9 px total).
- **Popup chrome**: inherits `Popup` (surface bg, 6 px radius, drop shadow, 4 px gap from anchor).
- **Cascading sub-menu**: opens with `PopupPlacement::Right` against the item's row rect.
