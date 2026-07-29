//! Multi-monitor enumeration and lookup helpers (Phase 2.3 / Ruling 1).
//!
//! Wraps the Win32 `EnumDisplayMonitors` / `MonitorFromPoint` /
//! `MonitorFromWindow` family in panic-free, allocation-bounded helpers. The
//! caller stays in `i32` device-pixel coordinates throughout — DPI scaling
//! belongs to a later wave (PHASE_2.3.1).
//!
//! Failure semantics: every public function is total. Lookups that miss
//! fall back to the primary monitor; `enumerate_monitors` returns an empty
//! `SmallVec` only if Win32 itself rejects the call (unobserved in practice).
//! Spec §11 forbids panic-shaped operations: no `unwrap`, `expect`, or
//! `panic!` outside `#[cfg(test)]`.

use bentodesk_zone::Zone;
use smallvec::SmallVec;
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

/// Inclusive-left / exclusive-right rectangle in virtual desktop coords. The
/// negative-coord case is real: Windows places secondary monitors to the
/// left of or above the primary by giving them negative origins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectI32 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectI32 {
    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// `[left, right) x [top, bottom)` containment — matches Win32 `PtInRect`
    /// semantics so a point on the bottom-right edge belongs to the
    /// neighbouring monitor, never to two monitors at once.
    pub const fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    const fn from_win32(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

/// One display monitor as reported by `GetMonitorInfoW`. Carries the opaque
/// HMONITOR handle so callers can re-feed it into Win32 APIs without a
/// second `EnumDisplayMonitors` round trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorInfo {
    pub hmonitor: HMONITOR,
    pub rect_screen: RectI32,
    pub rect_work: RectI32,
    pub is_primary: bool,
}

/// Sentinel monitor used when Win32 enumeration fails outright. Picked so
/// `contains_point` returns `false` for any real cursor — keeps callers
/// honest about the "no monitors" branch instead of pretending `(0,0)`
/// belongs to a synthetic display.
const FALLBACK_NO_MONITOR: MonitorInfo = MonitorInfo {
    hmonitor: core::ptr::null_mut(),
    rect_screen: RectI32 {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    },
    rect_work: RectI32 {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    },
    is_primary: true,
};

/// Enumerate every attached monitor. Order: primary first (when present),
/// then in the order Win32 reports them. The inline capacity (4) covers the
/// 99th-percentile workstation; spillover to the heap is acceptable for the
/// rare 5+ monitor setups.
pub fn enumerate_monitors() -> SmallVec<[MonitorInfo; 4]> {
    let mut sink: SmallVec<[MonitorInfo; 4]> = SmallVec::new();
    let lparam = (&mut sink) as *mut SmallVec<[MonitorInfo; 4]> as LPARAM;
    // SAFETY: `enum_proc` reads the SmallVec back via `lparam`; the call is
    // synchronous so the borrow lives for the duration of the FFI call.
    let _ok = unsafe { EnumDisplayMonitors(0 as HDC, core::ptr::null(), Some(enum_proc), lparam) };
    // Sort primary first so callers can index 0 for a "best guess" monitor.
    if let Some(idx) = sink.iter().position(|m| m.is_primary)
        && idx != 0
    {
        sink.swap(0, idx);
    }
    sink
}

/// Resolve the monitor containing `(x, y)`. Falls back to the primary when
/// the point is off all monitors (or `MonitorFromPoint` returns NULL).
pub fn monitor_from_point(x: i32, y: i32) -> MonitorInfo {
    let pt = POINT { x, y };
    // SAFETY: MonitorFromPoint is documented to never write through `pt`;
    // the call is total — NULL on miss, never a dangling handle.
    let h = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY) };
    info_for_handle(h).unwrap_or_else(primary_monitor)
}

/// Resolve the monitor containing `hwnd`'s majority area. Falls back to the
/// primary when `hwnd` is invalid or off-screen.
///
/// # Safety
///
/// `hwnd` must be a valid (alive) window handle. Callers passing a
/// destroyed HWND invoke documented Win32 UB (the kernel does its own
/// pointer check; this wrapper does not re-validate).
pub unsafe fn monitor_from_window(hwnd: HWND) -> MonitorInfo {
    // SAFETY: see public-fn doc — the HWND validity contract is the
    // caller's responsibility per Win32 documentation.
    let h = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
    info_for_handle(h).unwrap_or_else(primary_monitor)
}

/// Primary monitor. Always succeeds because Windows guarantees at least
/// one display; the fallback path keeps the function total even when
/// `EnumDisplayMonitors` mysteriously returns nothing.
pub fn primary_monitor() -> MonitorInfo {
    let monitors = enumerate_monitors();
    if let Some(m) = monitors.iter().find(|m| m.is_primary) {
        return *m;
    }
    if let Some(m) = monitors.first() {
        return *m;
    }
    FALLBACK_NO_MONITOR
}

/// Index of the monitor whose `rect_work` contains the zone's centre point.
/// Returns 0 (primary) when the zone is fully off every monitor's work
/// area, when `monitors` is empty, or when the centre lands exactly on a
/// boundary belonging to no monitor under the half-open rule.
///
/// Pure function — does not touch Win32. Callers responsible for passing
/// `enumerate_monitors()` (or a test fixture) as `monitors`.
pub fn zone_active_monitor_index(zone: &Zone, monitors: &[MonitorInfo]) -> usize {
    if monitors.is_empty() {
        return 0;
    }
    let cx = zone.x + zone.w / 2;
    let cy = zone.y + zone.h / 2;
    for (i, m) in monitors.iter().enumerate() {
        if m.rect_work.contains_point(cx, cy) {
            return i;
        }
    }
    0
}

/// Clamp a zone's `(x, y)` so its half-open rect `[x, x+w) × [y, y+h)`
/// overlaps at least one monitor's `rect_work`. Width / height are never
/// touched — only position. Pure function; allocation-free; safe to call
/// every WM_MOUSEMOVE in the drag handler (§10 hot-path discipline).
///
/// Behaviour matrix:
/// - Empty `monitors` slice → no-op (defensive; production callers pass
///   the cached `WindowState.monitors`, which is empty only in the brief
///   window between `WM_NCCREATE` and the first paint seed).
/// - Zero-or-negative-area zone (`w <= 0 || h <= 0`) → no-op (overlap is
///   undefined; punt rather than synthesize a position).
/// - Zone already overlaps any monitor's work area → no-op (1-px overlap
///   under half-open semantics counts as visible per the Win32 convention
///   `RectI32::contains_point` already encodes).
/// - Otherwise: pick the monitor whose work area is nearest to the zone
///   centre (Manhattan distance from centre to clamped centre — equivalent
///   to rect-to-rect L1 distance for non-overlapping cases) and translate
///   the zone position so its rect overlaps that monitor's work area by
///   exactly 1 px on each axis it had to move. Width / height unchanged.
///
/// The 1-px overlap convention matches half-open rect semantics: with
/// `monitor.rect_work = [0, 1920)` and zone width `200`, the legal
/// `zone.x` range that produces a non-empty intersection is
/// `[1 - 200, 1920 - 1] = [-199, 1919]` (so `zone.x = -199` overlaps
/// at column 0 only; `zone.x = 1919` overlaps at column 1919 only).
pub fn clamp_zone_to_monitors(zone: &mut Zone, monitors: &[MonitorInfo]) {
    if monitors.is_empty() {
        return;
    }
    if zone.w <= 0 || zone.h <= 0 {
        return;
    }
    // Already overlaps any monitor work area → done.
    if monitors
        .iter()
        .any(|m| zone_overlaps_rect(zone, &m.rect_work))
    {
        return;
    }
    // Need to move. Pick nearest monitor by L1 distance from zone centre
    // to the closest point of `rect_work`. Compute inline (no Vec / sort).
    let cx = zone.x + zone.w / 2;
    let cy = zone.y + zone.h / 2;
    let mut best_idx: usize = 0;
    let mut best_dist: i64 = i64::MAX;
    for (i, m) in monitors.iter().enumerate() {
        // Skip degenerate monitors (e.g. FALLBACK_NO_MONITOR sentinel) so
        // we never clamp into a zero-area target.
        if m.rect_work.width() <= 0 || m.rect_work.height() <= 0 {
            continue;
        }
        let clamped_cx = clamp_i32(cx, m.rect_work.left, m.rect_work.right - 1);
        let clamped_cy = clamp_i32(cy, m.rect_work.top, m.rect_work.bottom - 1);
        let dx = (cx - clamped_cx).unsigned_abs() as i64;
        let dy = (cy - clamped_cy).unsigned_abs() as i64;
        let dist = dx + dy;
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }
    // No usable monitor (all degenerate) → nothing safe to do; bail.
    if best_dist == i64::MAX {
        return;
    }
    let target = &monitors[best_idx].rect_work;
    // Solve for zone.x s.t. [zone.x, zone.x + w) ∩ [target.left, target.right)
    // is non-empty. Equivalent to zone.x ∈ [target.left - w + 1, target.right - 1].
    let min_x = target.left - zone.w + 1;
    let max_x = target.right - 1;
    let min_y = target.top - zone.h + 1;
    let max_y = target.bottom - 1;
    zone.x = clamp_i32(zone.x, min_x, max_x);
    zone.y = clamp_i32(zone.y, min_y, max_y);
}

/// Rescue an offscreen window. If the screen rect `(x, y, w, h)` overlaps
/// ANY monitor's `rect_work`, returns `(x, y)` unchanged. Otherwise picks
/// the nearest monitor by L1 distance from the window centre and returns a
/// `(x, y)` that makes the window overlap that monitor's work area. Size is
/// never inspected for change — the caller preserves it (e.g. `SWP_NOSIZE`).
///
/// Mirrors `clamp_zone_to_monitors` exactly, but operates on a raw screen
/// rect (top-left + size) instead of a `Zone`, returning the clamped
/// top-left rather than mutating in place. Panic-free (reuses `clamp_i32`,
/// never `Ord::clamp`).
///
/// Behaviour matrix:
/// - Empty `monitors` slice → no-op (returns `(x, y)`).
/// - Zero-or-negative-area window (`w <= 0 || h <= 0`) → no-op.
/// - Window already overlaps any monitor's work area → no-op (so a still-
///   visible window is never moved; only a window off ALL work areas is
///   rescued).
/// - Otherwise: translate the top-left so the window overlaps the nearest
///   monitor's work area by exactly 1 px on each axis it had to move.
pub fn clamp_window_to_monitors(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    monitors: &[MonitorInfo],
) -> (i32, i32) {
    if monitors.is_empty() {
        return (x, y);
    }
    if w <= 0 || h <= 0 {
        return (x, y);
    }
    // Already overlaps any monitor work area → leave it alone. A 1-px
    // overlap counts as visible under the half-open convention.
    if monitors
        .iter()
        .any(|m| rect_overlaps_rect(x, y, w, h, &m.rect_work))
    {
        return (x, y);
    }
    // Off all work areas. Pick nearest monitor by L1 distance from the
    // window centre to the closest point of `rect_work`. Inline (no Vec).
    let cx = x + w / 2;
    let cy = y + h / 2;
    let mut best_idx: usize = 0;
    let mut best_dist: i64 = i64::MAX;
    for (i, m) in monitors.iter().enumerate() {
        // Skip degenerate monitors (e.g. FALLBACK_NO_MONITOR sentinel) so
        // we never clamp into a zero-area target.
        if m.rect_work.width() <= 0 || m.rect_work.height() <= 0 {
            continue;
        }
        let clamped_cx = clamp_i32(cx, m.rect_work.left, m.rect_work.right - 1);
        let clamped_cy = clamp_i32(cy, m.rect_work.top, m.rect_work.bottom - 1);
        let dx = (cx - clamped_cx).unsigned_abs() as i64;
        let dy = (cy - clamped_cy).unsigned_abs() as i64;
        let dist = dx + dy;
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }
    // No usable monitor (all degenerate) → nothing safe to do; bail.
    if best_dist == i64::MAX {
        return (x, y);
    }
    let target = &monitors[best_idx].rect_work;
    let min_x = target.left - w + 1;
    let max_x = target.right - 1;
    let min_y = target.top - h + 1;
    let max_y = target.bottom - 1;
    (clamp_i32(x, min_x, max_x), clamp_i32(y, min_y, max_y))
}

/// Strictly pin a raw rect `(x, y, w, h)` fully inside the UNION bounding
/// rectangle of every non-degenerate monitor work area, returning the
/// clamped top-left. Width / height are never touched.
///
/// This is the LIVE-DRAG clamp (M4 zone gestures, ZRainbow ruling Option A —
/// STRICT INSET, UNION BOUNDS). Unlike `clamp_window_to_monitors` /
/// `clamp_zone_to_monitors` — which are rescue-only (they no-op while ≥1 px
/// still overlaps a monitor and only translate a fully-offscreen rect back) —
/// this helper pins the rect so it can NEVER overhang the OUTER desktop edge,
/// even by a single pixel. On a single monitor that is pixel-identical to
/// Tauri's `x_percent ∈ [0, 100 − capWidthPct]` clamp (`BentoZone.tsx`): the
/// dragged zone is held fully inside that monitor. On multi-monitor the zone
/// roams freely inside the combined outer rect and may straddle the seam
/// between adjacent monitors, but never exits the outer boundary.
///
/// Semantics (half-open convention `[x, x+w) ⊆ [union.left, union.right)`,
/// likewise Y):
/// - Empty `monitors` slice → no-op (returns `(x, y)`).
/// - All monitors degenerate (`width <= 0 || height <= 0`, e.g. the
///   `FALLBACK_NO_MONITOR` sentinel) → no-op (returns `(x, y)`). Degenerate
///   monitors are skipped when forming the union, mirroring the existing
///   clamps.
/// - Clamp range `x ∈ [union.left, union.right − w]`, `y ∈ [union.top,
///   union.bottom − h]`.
/// - Oversized rect (`w > union.width`) → pin to `union.left` (and likewise
///   Y to `union.top` when `h > union.height`). Achieved naturally by
///   `clamp_i32`'s `min > max` fold (`union.right − w < union.left`).
///
/// Pure + allocation-free + panic-free (uses `clamp_i32`, never
/// `Ord::clamp`). Safe to call every `WM_MOUSEMOVE` on the drag hot path
/// (§10).
pub fn clamp_rect_into_union_bounds(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    monitors: &[MonitorInfo],
) -> (i32, i32) {
    let Some(union) = union_work_bounds(monitors) else {
        return (x, y);
    };
    // Clamp range: x ∈ [union.left, union.right - w]. When w > union.width,
    // `union.right - w < union.left` so `clamp_i32`'s min>max fold pins to
    // `union.left` (the lower bound), exactly the oversized-rect contract.
    let nx = clamp_i32(x, union.left, union.right - w);
    let ny = clamp_i32(y, union.top, union.bottom - h);
    (nx, ny)
}

/// Union bounding rectangle of every NON-DEGENERATE monitor `rect_work`.
/// Returns `None` when `monitors` is empty or all entries are degenerate
/// (`width <= 0 || height <= 0`, e.g. the `FALLBACK_NO_MONITOR` sentinel),
/// so callers can no-op without synthesizing a zero-area target — matching
/// the degenerate-skip convention of `clamp_window_to_monitors`.
fn union_work_bounds(monitors: &[MonitorInfo]) -> Option<RectI32> {
    let mut acc: Option<RectI32> = None;
    for m in monitors {
        let r = &m.rect_work;
        if r.width() <= 0 || r.height() <= 0 {
            continue;
        }
        acc = Some(match acc {
            None => *r,
            Some(u) => RectI32 {
                left: u.left.min(r.left),
                top: u.top.min(r.top),
                right: u.right.max(r.right),
                bottom: u.bottom.max(r.bottom),
            },
        });
    }
    acc
}

/// `[x, x+w) × [y, y+h)` ∩ `r` non-empty? Half-open on all four sides to
/// match `RectI32::contains_point`. Screen-rect twin of `zone_overlaps_rect`.
#[inline]
fn rect_overlaps_rect(x: i32, y: i32, w: i32, h: i32, r: &RectI32) -> bool {
    let xr = x + w;
    let yb = y + h;
    x < r.right && xr > r.left && y < r.bottom && yb > r.top
}

/// `[zone.x, zone.x+w) × [zone.y, zone.y+h)` ∩ `r` non-empty? Half-open on
/// all four sides to match `RectI32::contains_point`.
#[inline]
fn zone_overlaps_rect(zone: &Zone, r: &RectI32) -> bool {
    let zr = zone.x + zone.w;
    let zb = zone.y + zone.h;
    zone.x < r.right && zr > r.left && zone.y < r.bottom && zb > r.top
}

/// Branchless `i32` clamp. `core::cmp::Ord::clamp` panics on `min > max`,
/// which the call sites above can in principle hit if a caller hands us a
/// pathological `MonitorInfo` (e.g. `right < left`); fold to a safe order
/// here so §11 panic-free discipline holds even on malformed inputs.
#[inline]
const fn clamp_i32(v: i32, lo: i32, hi: i32) -> i32 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

// -----------------------------------------------------------------------------
// internals
// -----------------------------------------------------------------------------

/// `EnumDisplayMonitors` callback. Reads the SmallVec back through the
/// `LPARAM` cookie and pushes a `MonitorInfo` per monitor. Returning
/// `TRUE` keeps enumeration going; we never short-circuit early.
unsafe extern "system" fn enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprc: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: `lparam` was constructed in `enumerate_monitors` from a `&mut
    // SmallVec` pointer that lives for the full FFI call.
    let sink = unsafe { &mut *(lparam as *mut SmallVec<[MonitorInfo; 4]>) };
    if let Some(info) = read_monitor_info(hmonitor) {
        sink.push(info);
    }
    TRUE
}

/// Wrap `GetMonitorInfoW` and lift the `MONITORINFO` into our typed view.
/// Returns `None` only when Win32 itself rejects the handle (NULL, freed,
/// or otherwise invalid).
fn read_monitor_info(hmonitor: HMONITOR) -> Option<MonitorInfo> {
    if hmonitor.is_null() {
        return None;
    }
    // SAFETY: MONITORINFO is plain-old-data with no Drop / no references;
    // zero-init is a documented Win32 pattern, and `cbSize` is set on the
    // very next line before any read.
    let mut mi: MONITORINFO = unsafe { core::mem::zeroed() };
    mi.cbSize = core::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: `mi` is fully initialised; `cbSize` set per docs.
    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut mi) };
    if ok == 0 {
        return None;
    }
    Some(MonitorInfo {
        hmonitor,
        rect_screen: RectI32::from_win32(mi.rcMonitor),
        rect_work: RectI32::from_win32(mi.rcWork),
        is_primary: (mi.dwFlags & MONITORINFOF_PRIMARY) != 0,
    })
}

/// Convenience: bypass `MonitorFromX` ⇒ `GetMonitorInfoW` short-circuit when
/// the lookup returned NULL.
fn info_for_handle(hmonitor: HMONITOR) -> Option<MonitorInfo> {
    if hmonitor.is_null() {
        None
    } else {
        read_monitor_info(hmonitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic monitor fixture — `clamp_window_to_monitors` never touches
    /// `hmonitor`, so a NULL sentinel handle is fine.
    fn fake(left: i32, top: i32, right: i32, bottom: i32, primary: bool) -> MonitorInfo {
        let rect = RectI32 {
            left,
            top,
            right,
            bottom,
        };
        MonitorInfo {
            hmonitor: core::ptr::null_mut(),
            rect_screen: rect,
            rect_work: rect,
            is_primary: primary,
        }
    }

    #[test]
    fn clamp_window_offscreen_is_rescued_onto_a_work_area() {
        let monitors = [fake(0, 0, 1920, 1080, true)];
        // Window stranded far off all monitors (monitor unplug scenario).
        let (nx, ny) = clamp_window_to_monitors(99999, 99999, 400, 300, &monitors);
        // Must have moved back so the window's half-open rect overlaps the
        // single monitor's work area [0,1920) x [0,1080).
        assert!(nx != 99999 || ny != 99999, "offscreen window must move");
        assert!(
            nx < 1920 && nx + 400 > 0 && ny < 1080 && ny + 300 > 0,
            "rescued window ({nx},{ny},400,300) must overlap work area [0,1920)x[0,1080)"
        );
    }

    #[test]
    fn clamp_window_already_visible_is_returned_unchanged() {
        let monitors = [fake(0, 0, 1920, 1080, true)];
        // Fully inside the work area — must be a no-op.
        assert_eq!(
            clamp_window_to_monitors(100, 100, 400, 300, &monitors),
            (100, 100)
        );
        // 1-px overlap on the right edge still counts as visible (half-open).
        assert_eq!(
            clamp_window_to_monitors(1919, 500, 400, 300, &monitors),
            (1919, 500)
        );
    }

    #[test]
    fn clamp_window_empty_or_degenerate_is_noop() {
        // Empty monitor list → no-op (defensive guard).
        assert_eq!(
            clamp_window_to_monitors(99999, 99999, 400, 300, &[]),
            (99999, 99999)
        );
        // Zero-area window → no-op even when offscreen.
        let monitors = [fake(0, 0, 1920, 1080, true)];
        assert_eq!(
            clamp_window_to_monitors(99999, 99999, 0, 0, &monitors),
            (99999, 99999)
        );
    }

    // ── clamp_rect_into_union_bounds (M4 live-drag strict inset) ──────────

    #[test]
    fn union_bounds_single_monitor_is_tauri_strict_pin() {
        // Single monitor [0,1920)x[0,1080); zone 400x300. STRICT: the zone
        // must be held FULLY inside — never overhanging the right/bottom
        // inset. This is the Tauri-parity case (x_percent ∈ [0, 100-cap]).
        let monitors = [fake(0, 0, 1920, 1080, true)];
        // Already fully inside → unchanged.
        assert_eq!(
            clamp_rect_into_union_bounds(100, 100, 400, 300, &monitors),
            (100, 100)
        );
        // Pushed past the right edge → pinned so right edge == 1920.
        assert_eq!(
            clamp_rect_into_union_bounds(1800, 100, 400, 300, &monitors),
            (1920 - 400, 100)
        );
        // Pushed past the bottom edge → pinned so bottom edge == 1080.
        assert_eq!(
            clamp_rect_into_union_bounds(100, 1000, 400, 300, &monitors),
            (100, 1080 - 300)
        );
        // Pushed past the top-left → pinned to (0, 0).
        assert_eq!(
            clamp_rect_into_union_bounds(-50, -50, 400, 300, &monitors),
            (0, 0)
        );
        // Max legal top-left: right/bottom edges flush with the work area.
        assert_eq!(
            clamp_rect_into_union_bounds(1520, 780, 400, 300, &monitors),
            (1520, 780)
        );
        // Just past max → pinned back to max (no overhang).
        assert_eq!(
            clamp_rect_into_union_bounds(1521, 781, 400, 300, &monitors),
            (1520, 780)
        );
    }

    #[test]
    fn union_bounds_two_side_by_side_monitors_roam_the_full_union() {
        // Two 1920x1080 monitors side by side → union [0,3840)x[0,1080).
        let monitors = [
            fake(0, 0, 1920, 1080, true),
            fake(1920, 0, 3840, 1080, false),
        ];
        // A zone can straddle the seam between the two monitors (x spanning
        // 1920) and stay legal — it never exits the outer edge.
        assert_eq!(
            clamp_rect_into_union_bounds(1800, 400, 400, 300, &monitors),
            (1800, 400)
        );
        // Roams to the far right: pinned so right edge == 3840 (outer edge).
        assert_eq!(
            clamp_rect_into_union_bounds(3700, 400, 400, 300, &monitors),
            (3840 - 400, 400)
        );
        // Roams off the left edge: pinned to union.left (0).
        assert_eq!(
            clamp_rect_into_union_bounds(-100, 400, 400, 300, &monitors),
            (0, 400)
        );
        // Vertical is still bounded by the single shared height.
        assert_eq!(
            clamp_rect_into_union_bounds(2000, 2000, 400, 300, &monitors),
            (2000, 1080 - 300)
        );
    }

    #[test]
    fn union_bounds_oversized_zone_pins_to_top_left() {
        // Zone WIDER (and taller) than the whole desktop → pin to (left, top).
        let monitors = [fake(0, 0, 1920, 1080, true)];
        assert_eq!(
            clamp_rect_into_union_bounds(500, 500, 4000, 2000, &monitors),
            (0, 0)
        );
        // Negative-origin monitor: union.left/top are negative, so the
        // oversized pin lands on the negative origin, not (0,0).
        let neg = [fake(-2000, -500, 1920, 1080, true)];
        assert_eq!(
            clamp_rect_into_union_bounds(0, 0, 9000, 9000, &neg),
            (-2000, -500)
        );
    }

    #[test]
    fn union_bounds_empty_or_degenerate_is_noop() {
        // Empty slice → no-op.
        assert_eq!(
            clamp_rect_into_union_bounds(5000, 5000, 400, 300, &[]),
            (5000, 5000)
        );
        // All-degenerate (zero-area work areas, e.g. FALLBACK sentinel) →
        // no-op: nothing safe to clamp into.
        let degenerate = [fake(0, 0, 0, 0, true), fake(10, 10, 10, 10, false)];
        assert_eq!(
            clamp_rect_into_union_bounds(5000, 5000, 400, 300, &degenerate),
            (5000, 5000)
        );
    }
}
