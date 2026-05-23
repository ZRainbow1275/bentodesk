# DragPreview — visual spec (snap.md)

- **Default size**: 64×64 px square.
- **Cursor offset**: +12 px right, -8 px up — keeps cursor unobstructed.
- **Surface**: `palette.surface` background, 8 px corner radius.
- **Opacity**: lerps 0 → 1 over `EaseOut` 80 ms (`FADE_IN_SECS`).
- **HWND backing**: `WindowKind::DragPreview` — transparent, topmost, layered, click-through.
- **Caller paints**: payload-specific visual (icon ghost, zone outline, item card snapshot).
- **End drag**: fades out 80 ms then `visible = false`.
