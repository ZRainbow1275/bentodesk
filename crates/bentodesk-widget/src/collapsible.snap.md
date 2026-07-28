# Collapsible — visual spec (snap.md)

- **Header**: 32 px tall row, `palette.surface` background, 6 px corner radius (top corners when expanded).
- **Header label**: `palette.text` color, default font size, 8 px padding.
- **Body**: clipped column below header; height = natural × expand_progress.
- **Animation**: `EaseInOut` 220 ms on toggle (`EXPAND_DURATION_SECS`).
- **Chevron**: caller draws indicator (typically rotated 0° → 90° tied to expand_progress).
- **Padding**: 8 px on all sides.
