# Dropdown — visual spec (snap.md)

- **Default trigger**: 200×32 px; 8 px horizontal, 6 px vertical padding.
- **Background**: `palette.surface_alt`; 4 px corner radius.
- **Border**: 1 px `palette.border`; `palette.accent` when popup open.
- **Trigger text**: `palette.text`; chevron icon on right edge.
- **Popup**: opens below trigger (flips on overflow); inherits Popup chrome (surface bg, drop shadow).
- **Popup row**: 28 px tall (`ROW_HEIGHT_PX`); hover bg = `palette.hover_overlay`.
- **Disabled option**: `palette.text_muted`, no hover, click is no-op.
- **Disabled trigger**: 50% alpha; open is no-op.
