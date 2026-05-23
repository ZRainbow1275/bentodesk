# DebugOverlay — visual snap

Compact HUD pinned to the top-right of the main window. Toggled by
Ctrl+Shift+D (or `Command::ToggleDebugOverlay`). Always-on-top child of
the main window — never gets its own HWND.

- **Position:** anchored top-right with 12 px margin from window edges.
- **Size:** 220 × 96 px (fixed — content fits without truncation).
- **Surface:** `Color { r: 0.0, g: 0.0, b: 0.0, a: 0.7 }` (translucent black,
  reads against any background), `BorderRadius::all(8.0)`, padding 8 px.
- **Typography:** monospace 11 px, `palette.text` (white).
- **Rows:** 3 lines, `gap = 4.0`:
  1. `format!("FPS: {fps:>3}")`
  2. `format!("RSS: {rss_mb:>4.1} MB")`
  3. `format!("Frame: {last_us:>4} µs")`

## Wire constants

- `OVERLAY_WIDTH = 220.0`, `OVERLAY_HEIGHT = 96.0`.
- `EDGE_MARGIN = 12.0`.
- `SAMPLE_WINDOW_FRAMES: usize = 60` — rolling FPS averages across the last
  60 frames so the readout doesn't jitter every frame.
- `RSS_SAMPLE_INTERVAL_MS: u32 = 500` — RSS poll cadence (cheaper than
  per-frame, the value barely moves between frames).

## State machine

`DebugOverlayState` carries `frame_times_us: SmallVec<[u32; 60]>`
(rolling buffer), `last_rss_mb: f32`, `last_rss_at_ms: u32`,
`current_now_ms: u32`. The `record_frame(us)` and
`record_rss_if_due(now_ms, rss_mb)` methods drive both readouts; the
shell calls them once per frame from the renderer's frame-end hook.

## Hibernation contract

Overlay is part of the main window — no separate swap chain. When the
main window hibernates, the overlay hibernates with it.
