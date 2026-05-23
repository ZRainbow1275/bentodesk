# Modal — visual spec (snap.md)

- **Scrim**: full-screen `palette.scrim` (≈ 50% black), alpha lerped via fade_anim.
- **Body**: centred in screen, default 16 px padding, 8 px corner radius, `palette.surface` background.
- **Body border**: 1 px `palette.border`.
- **Body shadow**: offset (0, +8), blur 32, `#00000099` — pronounced lift over the scrim.
- **Animation**: scrim + body opacity 0 → 1 over `EaseOut` 180 ms (`FADE_DURATION_SECS`); same on dismiss.
- **Dismiss policy**: `OutsideOnly` (default) — scrim click closes; body click never closes.
- **HWND backing**: full-screen `WindowKind::Settings`-style HWND with click-through scrim.
