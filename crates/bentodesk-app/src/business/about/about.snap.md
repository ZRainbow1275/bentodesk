# About — visual snap

Modal-style card hosted in `WindowKind::Settings` (or its own `WindowKind::About`
when shell adds the variant). Shown via `Command::OpenAbout`.

- **Window size:** 360 × 280 px (small modal — content is short).
- **Surface:** `theme::current().palette.surface`, `BorderRadius::all(12.0)`.
- **Padding:** 24 px on all sides.
- **Layout:** `Direction::Column`, `gap = 12.0` between rows.

## Rows (top → bottom)

1. **App icon** — 64×64 SVG (BentoDesk monogram), centred.
2. **Name + version** — "BentoDesk 2.0" in `palette.text` 18 px bold;
   `format!("v{} ({})", VERSION, BUILD_HASH)` in `palette.text_dim` 12 px below.
3. **Copyright** — "© 2026 BentoDesk Authors" in `palette.text_dim` 11 px.
4. **License link** — "MIT OR Apache-2.0" — opens external license file via
   `Command::OpenLicenseDoc`. Hover underline.
5. **Close button** — primary tint, bottom-right.

## Wire constants

- `VERSION` — pinned to `env!("CARGO_PKG_VERSION")` at compile time.
- `BUILD_HASH` — pinned to `env!("BENTO_BUILD_HASH")` if set, else `"dev"`.
- `WINDOW_WIDTH = 360.0`, `WINDOW_HEIGHT = 280.0`.

## Hibernation contract

The About window is short-lived — opened on user click, closed on dismiss.
No swap-chain hibernation needed (sub-second visibility window).
