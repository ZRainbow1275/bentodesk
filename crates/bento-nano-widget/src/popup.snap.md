# Popup — visual spec (snap.md)

- **Surface**: `palette.surface` background, 1 px `palette.border`, 6 px corner radius.
- **Padding**: 8 px on all sides (content slot).
- **Shadow**: offset (0, +4), blur 16, color `#00000066` — drop shadow under the body.
- **Anchor gap**: `POPUP_GAP_PX = 4` between anchor edge and popup body.
- **Placement**: prefers `Bottom`; flips to opposite side on overflow; clamps to screen if both sides overflow.
- **HWND backing**: opens a `WindowKind::Tooltip` or `Popup` HWND via platform layer; T-099 hibernates swap chain when `visible = false`.
- **Animation**: scale 0.96 → 1.0 + opacity 0 → 1 over 120 ms `EaseOut` (driven by caller).
