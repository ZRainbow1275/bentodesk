# MiniBar — Visual Fidelity Snapshot

Per Ruling R3 (master-decomposition §11). Pin the as-shipped 1.x visual
contract so the user can spot-check intent in Phase 6 without running both
apps.

## Source

`bentodesk/src/components/MiniBar.css` + the implicit MiniBar HWND created
by the 1.x `pin_zone_as_minibar` IPC.

## Geometry

- Default size: **280 × 80** device-independent pixels at 96 DPI baseline
  (matches `WindowKind::MiniBar` `default_size`).
- Corner radius: **12 px** (all four corners).
- Inset padding: **12 px** uniform.

## Layout

Row, left → right:

1. Zone icon (24 × 24 SVG, `palette.text` tint).
2. Zone label — truncated to 18 chars + `…` ellipsis. `SmolStr` keeps it
   inline (≤ 22 byte cap fits comfortably).
3. Spacer (flex 1).
4. Unpin affordance — `IconButton` with the inline 24×24 "pin-off" Lucide
   glyph (path: `M2 12L10 4M14 12V20H10V18M22 12L14 4M22 22L2 2`).

## Colours

- Background: `palette.surface` (DARK = 0x18181CCC, LIGHT = 0xFFFFFFFF).
- Label text: `palette.text` (DARK = 0xE0E0E6FF).
- Icon tint: `palette.text` (matches label).
- Unpin button hover overlay: `palette.hover_overlay` (DARK = 0xFFFFFF14).

## Hover / animation

- Unpin button: 150 ms ease-out hover overlay alpha lift (per
  `IconButton::HOVER_DURATION_SECS`).
- No bar-level hover effect — the bar is always-on-top tool window.

## Window class

- `WindowKind::MiniBar` → `(WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
  WS_POPUP | WS_VISIBLE)` per `bentodesk-platform::window::ex_style_for`.
- Layered (NOT NoRedirectionBitmap) — the §4.1 mutex makes this exclusive.
- NoActivate so pinning a zone doesn't steal focus from whatever app the
  user was working in.

## Hibernation contract (§11 R5)

- When MiniBar HWND receives `WM_SHOWWINDOW(SW_HIDE)` (driven by
  `Command::UnpinMinibar` or by the OS on app-suspend), the slot's
  `set_visible(false, …)` flips `pending_hibernate`.
- 500 ms later the next paint pump's `flush_hibernation` calls
  `Renderer::release_swap_chain` — the MiniBar's ~180 KB DXGI backbuffer
  goes back to the OS, leaving only the DComp visual tree (~few KB).
- On `Command::PinZoneAsMinibar` (re-pin) or `WM_SHOWWINDOW(SW_SHOW)`,
  the wndproc's WM_PAINT arm calls `Renderer::ensure_swap_chain` first;
  the chain is rebuilt before the next frame draws.

## Cap (§11 R7)

- `MiniBarRoster::pin` refuses the 9th simultaneous pin with
  `MiniBarError::CapReached`. Mirror of the shell registry's hard cap;
  the user-space refusal lets the UI show a "you can pin up to 8 zones"
  toast instead of silently no-op'ing in the registry.

## Smoke verification

`mod tests` in `mod.rs` proves:

- `MiniBarController::hide()` calls `release_swap_chain` exactly once and
  flips `is_resident` to `false`.
- `MiniBarController::show()` after `hide()` calls `ensure_swap_chain(280, 80)`
  exactly once, flips `is_resident` back to `true`.
- Both calls are idempotent (`hide()` twice = one release; `show()` while
  resident = zero ensures).
- `ensure_swap_chain` failure surfaces as `MiniBarError::SwapChainEnsure`
  without leaving the controller in an inconsistent visibility state.
- `MiniBarRoster` enforces the §11 R7 cap at the source.
