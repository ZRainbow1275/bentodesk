# List — visual spec (snap.md)

- **Direction**: vertical column; default 0 px gap, no padding.
- **Background**: transparent — caller paints surface.
- **Hover row**: `palette.hover_overlay` background under `hovered` index.
- **Selected row**: `palette.selection` background (semi-translucent accent).
- **Sizing**: `width=Auto` / `height=Auto` — fills parent.
- **No virtualisation**: every item lays out every frame; switch to `VirtualList` >100 entries.
