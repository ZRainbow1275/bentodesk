# Toggle — visual spec (snap.md)

- **Track**: 32×18 px, fully-rounded ends (radius = 9 px = height/2).
- **Track colors**: off = `palette.surface_alt`, on = `palette.accent` (lerped over thumb_anim).
- **Thumb**: 14×14 px circle, white fill (`Color::WHITE`), 2 px inset from track edge.
- **Thumb travel**: x = 2 → 16 (DIPs inside the 32-wide track).
- **Animation**: `EaseOut` 180 ms on toggle (`TOGGLE_DURATION_SECS`).
- **Disabled**: 50% alpha overlay + toggle is no-op.
