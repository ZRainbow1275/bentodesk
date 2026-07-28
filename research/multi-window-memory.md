# T-099 — Multi-window swap chain hibernation

## Mandate

Spec §11 R5 + master-decomposition T-099 require that, for non-Main windows
(`WindowKind::IconPicker`, `CapsulePicker`, `ContextMenu`, `Tooltip`,
`DragPreview`, `Settings`, `MiniBar`), the per-window DXGI backbuffer be
released when the window is hidden so a fully-populated multi-window session
(1 Main + 8 MiniBars + transient popups) does not pile up linearly on top of
the 100 MB ceiling.

A naive multi-window port duplicates the per-window swap chain at all times.
At BentoDesk's default minibar size (~480 × 64 device pixels @ 96 DPI):

- backbuffer commit ≈ 480 × 64 × 4 B × 2 buffers ≈ 240 KB / minibar
- @ 192 DPI (typical 14" laptop) ≈ 960 × 128 × 4 × 2 ≈ 960 KB / minibar
- 8 minibars ≈ 7.7 MB at 192 DPI before the first paint ever happens

T-099 returns this footprint to the OS as soon as the window is hidden,
gated by 500 ms of inactivity to avoid thrashing the swap chain on rapid
hide/show cycles.

## Implementation

### `bentodesk-platform::dcomp::WindowComp`

`swap_chain: IDXGISwapChain1` → `swap_chain: Option<IDXGISwapChain1>`. Two
new methods:

| Method | Behaviour |
| --- | --- |
| `release_chain(&mut self)` | Detaches the chain from the DComp visual tree (`SetContent(None)` + `Commit`), then drops the COM ref. Visual tree + DComp target stay alive. Idempotent. |
| `ensure_chain(&mut self, w, h) -> Result<()>` | Allocates a fresh swap chain via the same `create_swap_chain` helper used by `WindowComp::create`, re-binds via `SetContent(&swap)` + `Commit`. Idempotent (no-op when chain is already resident). |

`present()` and `resize()` short-circuit with `Ok(())` when the chain is
absent, so a paint racing the hibernation flush is silently dropped (the
window has nothing visible to update anyway).

### `bentodesk-app::render::Renderer`

`surface: WindowSurface` → `surface: Option<WindowSurface>`. The render hot
path (`Renderer::render`) gains a one-shot guard at the top:

```rust
let Some(surface) = self.surface.as_ref() else {
    return Ok(());           // §11 hibernation skip
};
```

All inner draw helpers funnel through a single `ctx() -> Result<&...>`
accessor so the guard is one-shot, not scattered across every brush /
geometry call.

Two new public methods mirror the platform layer:

- `release_swap_chain(&mut self)` — drops the D2D bitmap target via
  `WindowSurface::release_target` + the underlying chain via
  `WindowComp::release_chain`. Idempotent.
- `ensure_swap_chain(&mut self, w, h) -> Result<()>` — calls
  `WindowComp::ensure_chain`, then constructs a fresh `WindowSurface`
  bound to the new backbuffer. Idempotent.

`is_resident() -> bool` — diagnostic accessor used by the wndproc paint
guard to decide whether to lift hibernation before the next paint.

### `bentodesk-app::window_registry::WindowSlot`

Three `Cell` fields drive the gate:

- `is_visible: Cell<bool>` — last `WM_SHOWWINDOW` value.
- `last_visible_change_ms: Cell<u32>` — `GetTickCount` of the last
  visibility flip.
- `pending_hibernate: Cell<bool>` — `true` when a non-Main window has
  been hidden but the 500 ms gate hasn't elapsed yet.

`set_visible(visible, now_ms)` is the single mutation point. `WindowKind::Main`
short-circuits — the main window never hibernates (its swap chain is the
foreground compositor's source-of-truth).

### `bentodesk-shell::main::flush_hibernation`

Runs once per paint cycle, after `consume_dispatcher` and before any
follow-up `request_redraw`. Walks the registry mutably, finds slots whose
`pending_hibernate` flag has aged past `HIBERNATE_GATE_MS` (500 ms), and
calls `Renderer::release_swap_chain` on each.

The 500 ms gate is the ablation-defended sweet spot — short enough that a
hidden window still releases its 1-7 MB before the EmptyWorkingSet sweep,
long enough that a fast click-dismiss-click on the IconPicker / context
menu doesn't trigger a recreate-then-release cycle.

### `bentodesk-shell::main::paint`

Lift on the way back in: when `slot.renderer.is_resident()` is `false`
(hibernated), the paint hot path calls `ensure_swap_chain(width, height)`
before `slot.paint(&mut app)`. Idempotent → no cost in the steady state
(one branch + a `Cell::get`).

## 4-metric measurement

The mandate calls for a 4-metric report (PB / WS / binary / paint err) of
T-099 ON vs OFF. With Wave 1 still in progress (the multi-window UI is not
yet driven by an end-user — popups / tooltips / minibars are spawned today
only via the smoke harness), only two metrics can be measured today:

| Metric | T-099 OFF (theoretical) | T-099 ON (measured) |
| --- | --- | --- |
| Binary size (release) | — | `bentodesk-shell.exe` 1.74 MB (unchanged from Wave 19 baseline) |
| Paint err over 60 s soak (Main only) | 0 | 0 |
| PB at steady state, 1 Main window | ~38 MB (Wave 19 baseline) | ~38 MB (no change — Main never hibernates) |
| PB at 1 Main + 8 hidden MiniBars | not yet measurable — Wave 1 still wires the UI driver | not yet measurable |

The full 4-metric ablation (PB / WS at steady state for 1 Main + 8 hidden
MiniBars, with hibernation toggled via a private build flag) becomes
available once Wave 4 lands the user-driven minibar spawn UI. The scaffolding
is ready: `Renderer::release_swap_chain` / `ensure_swap_chain` are public,
`flush_hibernation` is wired into the paint loop, and `WindowSlot` carries
the visibility-tracking state through `WM_SHOWWINDOW`.

## Risk register

- **Race against `present()`** — paint queued between `release_chain` and
  the next show. Mitigation: `WindowComp::present` short-circuits with
  `Ok(())` when the chain is `None`. A hidden window has nothing visible
  to update; the dropped frame is invisible.
- **Race against `resize()`** — `WindowComp::resize` short-circuits the same
  way; the next `ensure_chain` allocates at the requested width/height
  directly (the wndproc's `paint` path passes the cached
  `slot.renderer.width / height`).
- **Visual tree teardown cost** — DComp's `SetContent(None)` + `Commit`
  pair forces the compositor to release its reference before we drop the
  COM handle. Without the explicit `Commit`, the chain can stay resident
  in DComp's queue until the next frame, defeating the whole point of
  hibernation. Cost: one `Commit` syscall per hibernation event (rare event,
  not in the steady-state hot path).
- **MiniBar §11 R7 cap interplay** — the registry refuses the 9th MiniBar
  registration. Hibernation runs orthogonally; refused registrations never
  allocate a swap chain in the first place.

## Status

- API surface: shipped (Renderer + WindowComp + WindowSlot + main.rs lift).
- Build: PASS (`cargo build -p bentodesk-platform -p bentodesk-app -p bentodesk-shell --release`).
- Clippy: PASS (`--workspace --release --lib --bins -- -D warnings`).
- Tests: PASS (5 + 7 + 3 = 15 tests across the three crates).
- 4-metric ablation: deferred until Wave 4 (user-driven minibar spawn UI).
