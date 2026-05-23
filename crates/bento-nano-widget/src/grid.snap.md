# Grid — visual spec (snap.md)

- **Direction**: row-major `Direction::Grid { columns }`.
- **Default gap**: 8 px between cells (both axes).
- **Cell width**: `(parent_width - (columns-1) * gap) / columns`.
- **Cell height**: per-child requested; `Auto` falls back to cell width (square).
- **Background**: transparent — caller paints.
- **Sizing**: `width=Auto` / `height=Auto` — fills parent.
