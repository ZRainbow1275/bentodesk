# Tab — visual spec (snap.md)

- **Header height**: 36 px (`HEADER_HEIGHT_PX`).
- **Item padding**: 12 px horizontal, 6 px vertical; 4 px gap between items.
- **Header text**: inactive = `palette.text_muted`, active = `palette.text`.
- **Underline**: 2 px thick (`UNDERLINE_THICKNESS_PX`), 1 px radius (full pill), `palette.accent` color.
- **Underline travel**: x-offset lerped via `EaseOut` 200 ms (`UNDERLINE_DURATION_SECS`).
- **Underline width**: matches active item's width (sized per-frame from `active_underline_width()`).
- **Content swap**: caller subscribes to `Signal<u32>` and rebuilds the body subtree on dirty.
