# Slider — visual spec (snap.md)

- **Track**: 200 px wide × 4 px tall, fully-rounded ends (radius = 2 px).
- **Track colors**: unfilled = `palette.surface_alt`, filled portion left of thumb = `palette.accent`.
- **Thumb**: 16 px circle, white fill, centered vertically on track, 2 px inset from track endpoints.
- **Hover halo**: ring around thumb, fades in 0 → 1 over `EaseOut` 150 ms (`HOVER_DURATION_SECS`).
- **Travel**: thumb x = 8 → 192 (DIPs); value lerped to `[0, 1]`.
- **Disabled**: 50% alpha; drag/set/hover are no-ops.
