# GridLayout — visual spec (snap.md)

- **Direction**: row-major `Direction::Grid { columns }`.
- **Default gap**: 0 px (caller sets via `with_gap`).
- **No chrome**: zero background, zero border — purely structural.
- **Children own visuals**: tiles / cards paint their own surface.
- **Sizing**: `width=Auto` / `height=Auto` — fills parent.
