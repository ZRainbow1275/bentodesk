# Checkbox — visual spec (snap.md)

- **Size**: 16x16 px (matches React `w-4 h-4`).
- **Corner radius**: 3 px on all four corners.
- **Box colors**: unchecked = `palette.surface_alt`, checked = `palette.accent`.
- **Border**: 1px `palette.border` (unchecked); becomes `accent` when fill_progress > 0.5.
- **Check mark**: white stroke, lerped from alpha 0 → 1 over fill_anim, drawn ~60% inset.
- **Animation**: `EaseOut` 120 ms on toggle (constant `CHECK_DURATION_SECS`).
- **Disabled**: 50% alpha overlay + toggle is no-op; cursor not changed (caller).
