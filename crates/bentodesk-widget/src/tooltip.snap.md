# Tooltip — visual spec (snap.md)

- **Default size**: 200×32 px (content slot — auto-shrinks to text width when DWrite measures the label).
- **Padding**: 8 px horizontal, 4 px vertical.
- **Surface**: inherits Popup chrome (`palette.surface` background, 6 px radius, drop shadow).
- **Show delay**: 500 ms (`SHOW_DELAY_SECS`) — Material standard.
- **Hide grace**: 100 ms (`HIDE_DELAY_SECS`) — survives sibling pointer-leave/enter.
- **Animation**: opacity 0 → 1 over 120 ms `EaseOut` on show; reverse on hide.
- **Placement**: defaults to `Bottom`; flips on overflow via Popup placement.
