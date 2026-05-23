# Radio — visual spec (snap.md)

- **Size**: 16×16 px circle (radius = 8 px).
- **Ring**: 1.5 px outer stroke. Color unselected = `palette.border`, selected = `palette.accent`.
- **Inner dot**: filled circle at 8×8 px (50% of outer), color = `palette.accent`.
- **Dot animation**: scale 0 → 1 + opacity 0 → 1 over `EaseOut` 140 ms (`SELECT_DURATION_SECS`).
- **Group semantics**: `group_id: SmolStr` + `value_id: u32`; only the matching value renders the dot.
- **Disabled**: 50% alpha; click is no-op; cursor not changed (caller).
