//! `MiniBar` — pinned-zone floating bar (T-078 + §11 R7 hibernation consumer).
//!
//! 1.x source: `bentodesk/src/components/MiniBar.css` + the implicit minibar
//! HWND created by `pin_zone_as_minibar`. Shape: a 280×80 always-on-top tool
//! window carrying the pinned zone's icon + label, with a small "unpin"
//! affordance on the right edge. Visual fidelity reference: `minibar.snap.md`.
//!
//! # Why this module is more than a descriptor
//!
//! Phase 0 closure measured one Main-window Private Bytes baseline at 96.10
//! MB; the §D PRD ceiling is 100 MB cold. With 8 pinned MiniBars each holding
//! a 280×80×4×2 ≈ 180 KB DXGI backbuffer plus DComp visuals (~1.2 MB total per
//! window), the worst-case "1 Main + 8 hidden MiniBars" steady state would
//! commit ≈ 96 + 8 × 1.2 = 105.6 MB — over budget. T-099 hibernation closes
//! that gap by releasing each hidden MiniBar's swap chain, dropping the
//! per-instance cost back to ~0 MB resident.
//!
//! `MiniBarController` is the trigger logic that wires the user-visible
//! show/hide actions (`MiniBar::pin` / `MiniBar::unpin` from the
//! 1.x `pin_zone_as_minibar` / `unpin_minibar` IPC contracts) onto the
//! [`HibernationGate`] surface. The actual swap-chain release/ensure lives
//! in `bentodesk-app::Renderer::release_swap_chain` /
//! `ensure_swap_chain`; the gate trait keeps this widget crate from pulling
//! the `app` crate in (cycle).
//!
//! # §11 R7 cap
//!
//! The shell registry already refuses the 9th `WindowKind::MiniBar`
//! registration. This module re-asserts the cap in user-space so a
//! `MiniBarRoster::pin` call returns `Err(MiniBarError::CapReached)` *before*
//! any Win32 work is requested — a soft pre-check prevents the user from
//! seeing the no-op pin behaviour the registry would silently produce.
//!
//! # Spec references
//! * §1 100 MB cold ceiling (governs by Ruling R5).
//! * §10 hot-path no-alloc — `MiniBarRoster` keeps the inline buffer at the
//!   §11 R7 cap of 8 entries.
//! * §11 no `panic!` / `unwrap` — every error path returns `MiniBarError`.
//! * §15 module ≤800 LOC (this file is ~280 LOC including tests).
//! * §17 no stubs — `HibernationGate` is fully exercised by the smoke test
//!   below using a fake `RecordingGate` that records every release/ensure
//!   call so the contract is verifiable without a live HWND.

use core::fmt;

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::tokens::{PaletteTauri, RadiusTauri, SpacingTauri};
use bentodesk_style::{BorderRadius, Color, Edges, Length, Rect, Size};
use bentodesk_theme::{self as theme, PaletteTokens, RadiusTokens, SpacingTokens};
use smallvec::SmallVec;
use smol_str::SmolStr;

use bentodesk_widget::IconButton;

/// `WindowKind::MiniBar` cap re-asserted at the user-space layer per §11 R7.
/// Mirrors the shell's `WindowRegistry` cap so UI can surface a toast
/// instead of a silent registry no-op when the user pins a 9th minibar.
pub const MAX_MINIBARS: usize = 8;
pub const MINIBAR_SOURCE_MAX_ITEMS: usize = 16;
const MINIBAR_ITEM_SLOT_SIZE: f32 = 32.0;
const MINIBAR_ITEM_SLOT_GAP: f32 = 4.0;

// -----------------------------------------------------------------------------
// Hibernation gate — the one trait that lets MiniBarController drive the
// per-window swap-chain release/ensure path without depending on
// `bentodesk-app::Renderer`.
// -----------------------------------------------------------------------------

/// Per-MiniBar-window hibernation contract. Implemented by
/// `bentodesk-app::Renderer` (the production wiring) and by
/// `RecordingGate` (the smoke test fake) — see the test below.
///
/// Method semantics mirror `Renderer`:
///   * [`release_swap_chain`] is idempotent — drops the DXGI backbuffer +
///     D2D surface so a hidden MiniBar contributes ~0 MB resident.
///   * [`ensure_swap_chain`] is also idempotent — rebuilds at `width × height`
///     when previously released; no-op when already resident.
///   * [`is_chain_resident`] mirrors `Renderer::is_resident`.
///
/// All three calls are synchronous + run on the UI thread — they never
/// allocate on the hot path beyond what the underlying COM allocations
/// already require for chain rebuild (one-shot, on user-initiated
/// show/hide; not per-frame).
///
/// [`release_swap_chain`]: HibernationGate::release_swap_chain
/// [`ensure_swap_chain`]: HibernationGate::ensure_swap_chain
/// [`is_chain_resident`]: HibernationGate::is_chain_resident
pub trait HibernationGate {
    /// Surface a release request. Idempotent — a second call when the chain
    /// is already absent is a no-op and must NOT error.
    fn release_swap_chain(&mut self);
    /// Surface an ensure request. Returns `Err` only when the underlying
    /// platform swap-chain rebuild fails (rare — D3D device lost / OOM);
    /// idempotent when already resident.
    fn ensure_swap_chain(&mut self, width: u32, height: u32) -> Result<(), MiniBarError>;
    /// Whether the chain is currently resident. Diagnostics + the smoke
    /// test read this to verify the gate fired.
    fn is_chain_resident(&self) -> bool;
}

/// Hand-rolled (no thiserror — §8.1) error returned from MiniBar lifecycle
/// + hibernation operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniBarError {
    /// `MiniBarRoster::pin` rejected because §11 R7 cap reached.
    CapReached,
    /// `MiniBarRoster::pin` rejected because the supplied id was already pinned.
    AlreadyPinned,
    /// `MiniBarRoster::unpin` rejected because the id wasn't pinned.
    NotFound,
    /// `HibernationGate::ensure_swap_chain` returned a platform error;
    /// the inner `&'static str` is the underlying ctx string from
    /// `PlatformError`. Surface text only — no source chain — keeps
    /// the type `Copy`-friendly + free of `Box<dyn Error>` (§10).
    SwapChainEnsure(&'static str),
}

impl fmt::Display for MiniBarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapReached => f.write_str("MiniBar cap reached (max 8 pinned per §11 R7)"),
            Self::AlreadyPinned => f.write_str("zone already pinned as a MiniBar"),
            Self::NotFound => f.write_str("zone is not pinned as a MiniBar"),
            Self::SwapChainEnsure(ctx) => write!(f, "MiniBar swap chain ensure failed: {ctx}"),
        }
    }
}

impl core::error::Error for MiniBarError {}

// -----------------------------------------------------------------------------
// MiniBar widget descriptor — POD layout source for the floating bar visual.
// -----------------------------------------------------------------------------

/// Floating-bar widget descriptor. One per pinned zone. Rendered in a
/// dedicated `WindowKind::MiniBar` HWND (280×80 default per `default_size`).
///
/// The widget itself is layout + visual identity: icon + truncated label +
/// unpin affordance. The lifecycle (pin/unpin, hibernation) is owned by
/// [`MiniBarRoster`] and [`MiniBarController`] respectively.
#[derive(Debug, Clone)]
pub struct MiniBar {
    /// SVG path for the zone icon (24×24 viewbox per `IconButton` convention).
    pub icon_svg_path: &'static str,
    /// Truncated zone label — 1.x truncates to 18 chars + ellipsis. Keeps
    /// `SmolStr` so the label fits inline (§10 hot-path no heap).
    pub label: SmolStr,
    /// Background fill — `palette.surface` per Wave B/T-004 ruling.
    pub background: Color,
    /// Border radius — 12 px matches 1.x `MiniBar.css` rounded corners.
    pub border_radius: BorderRadius,
    /// Inset padding around the icon + label row.
    pub padding: Edges,
    /// Width (device-independent pixels) — typically `Length::Px(280.0)`.
    pub width: Length,
    /// Height — typically `Length::Px(80.0)`.
    pub height: Length,
    /// Embedded "unpin" affordance — emits the dispatcher event id passed in
    /// by the constructor (mapped to `Command::UnpinMinibar(zone_id)` by the
    /// caller's match table).
    pub unpin_button: IconButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniBarHit {
    Body,
    Item(usize),
    Unpin,
}

impl MiniBar {
    /// Default: 280×80 dark surface, 12px radius, 12px inset padding,
    /// pre-wired unpin button using the supplied click event id.
    ///
    /// `icon_svg_path` is `&'static str` — icons live in the binary as
    /// compile-time SVG path literals (see `bentodesk-app::business::icons`
    /// once T-079 lands).
    pub fn new(
        icon_svg_path: &'static str,
        label: impl Into<SmolStr>,
        unpin_event_id: u32,
    ) -> Self {
        let palette = theme::current().palette;
        Self {
            icon_svg_path,
            label: label.into(),
            background: palette.surface,
            border_radius: BorderRadius::all(12.0),
            padding: Edges::all(12.0),
            width: Length::Px(280.0),
            height: Length::Px(80.0),
            unpin_button: IconButton::new(MINIBAR_UNPIN_PATH, unpin_event_id),
        }
    }

    /// Construct using explicit active theme tokens instead of the process
    /// default theme. This keeps runtime theme switches visible in the native
    /// MiniBar HWND even when the stored session was created before the switch.
    pub fn from_tokens(
        icon_svg_path: &'static str,
        label: impl Into<SmolStr>,
        unpin_event_id: u32,
        palette: PaletteTokens,
        radius: RadiusTokens,
        spacing: SpacingTokens,
    ) -> Self {
        Self::new(icon_svg_path, label, unpin_event_id).with_tokens(palette, radius, spacing)
    }

    /// Return a themed copy while preserving non-theme descriptor state such as
    /// event ids, icon path, dimensions, and button animation progress.
    pub fn with_tokens(
        mut self,
        palette: PaletteTokens,
        radius: RadiusTokens,
        spacing: SpacingTokens,
    ) -> Self {
        self.background = palette.surface;
        self.border_radius = radius.xl;
        self.padding = Edges::all(spacing.lg);
        self.unpin_button.tint = palette.text;
        self.unpin_button.hover_background = palette.hover_overlay;
        self
    }

    /// Apply Wave B Tauri SSoT tokens to the MiniBar visual chrome.
    ///
    /// Token mapping (Wave A `minibar.md` + Wave B `token-mapping.md`):
    /// - background ← `minibar_gradient_top` (top stop of the unique
    ///   `linear-gradient(rgba(18,22,34,0.82), rgba(14,16,26,0.72))`
    ///   gradient — D2D renders a solid fill today; gradient layering is
    ///   tracked as a Wave F follow-up).
    ///   V21-C2: renderer now consumes the live palette's top + bottom stops;
    ///   this field remains the legacy/fallback top-stop descriptor.
    /// - border_radius ← `RADIUS.minibar` (14 px — Wave A flagged gap).
    /// - padding ← `SPACING.lg` (16 px) — mirrors the legacy lg slot.
    /// - unpin button tint ← `text_primary`; hover bg ← `surface_hover`.
    pub fn with_tauri_tokens(
        mut self,
        palette: PaletteTauri,
        radius: RadiusTauri,
        spacing: SpacingTauri,
    ) -> Self {
        self.background = palette.minibar_gradient_top;
        self.border_radius = BorderRadius::all(radius.minibar);
        self.padding = Edges::all(spacing.lg);
        self.unpin_button.tint = palette.text_primary;
        self.unpin_button.hover_background = palette.surface_hover;
        self
    }
}

impl LayoutSource for MiniBar {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Row: [icon] [label] [spacer] [unpin]. Children are appended by
            // the renderer composer in declaration order.
            direction: Direction::Row,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

pub fn minibar_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width.max(1.0),
        height: viewport.height.max(1.0),
    }
}

pub fn minibar_icon_rect(viewport: Size, bar: &MiniBar) -> Rect {
    let panel = minibar_panel_rect(viewport);
    let size = 24.0;
    Rect {
        x: panel.x + bar.padding.left,
        y: panel.y + (panel.height - size) * 0.5,
        width: size,
        height: size,
    }
}

pub fn minibar_unpin_rect(viewport: Size, bar: &MiniBar) -> Rect {
    let panel = minibar_panel_rect(viewport);
    let size = bar.unpin_button.size;
    Rect {
        x: panel.right() - bar.padding.right - size,
        y: panel.y + (panel.height - size) * 0.5,
        width: size,
        height: size,
    }
}

pub fn minibar_label_rect(viewport: Size, bar: &MiniBar) -> Rect {
    let icon = minibar_icon_rect(viewport, bar);
    let unpin = minibar_unpin_rect(viewport, bar);
    let x = icon.right() + 10.0;
    Rect {
        x,
        y: icon.y + 1.0,
        width: (unpin.x - x - 10.0).max(1.0),
        height: icon.height,
    }
}

pub fn minibar_item_capacity(viewport: Size, bar: &MiniBar) -> usize {
    let icon = minibar_icon_rect(viewport, bar);
    let unpin = minibar_unpin_rect(viewport, bar);
    let available = (unpin.x - icon.right() - 14.0).max(0.0);
    let per_item = MINIBAR_ITEM_SLOT_SIZE + MINIBAR_ITEM_SLOT_GAP;
    ((available + MINIBAR_ITEM_SLOT_GAP) / per_item).floor() as usize
}

pub fn minibar_item_rect(viewport: Size, bar: &MiniBar, index: usize) -> Option<Rect> {
    if index >= MINIBAR_SOURCE_MAX_ITEMS || index >= minibar_item_capacity(viewport, bar) {
        return None;
    }
    let icon = minibar_icon_rect(viewport, bar);
    let panel = minibar_panel_rect(viewport);
    let x = icon.right() + 10.0 + index as f32 * (MINIBAR_ITEM_SLOT_SIZE + MINIBAR_ITEM_SLOT_GAP);
    let y = panel.y + (panel.height - MINIBAR_ITEM_SLOT_SIZE) * 0.5;
    Some(Rect {
        x,
        y,
        width: MINIBAR_ITEM_SLOT_SIZE,
        height: MINIBAR_ITEM_SLOT_SIZE,
    })
}

pub fn minibar_hit_test(viewport: Size, bar: &MiniBar, x: f32, y: f32) -> Option<MiniBarHit> {
    minibar_hit_test_with_items(viewport, bar, 0, x, y)
}

pub fn minibar_hit_test_with_items(
    viewport: Size,
    bar: &MiniBar,
    item_count: usize,
    x: f32,
    y: f32,
) -> Option<MiniBarHit> {
    let point_in =
        |rect: Rect| x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom();
    if point_in(minibar_unpin_rect(viewport, bar)) {
        return Some(MiniBarHit::Unpin);
    }
    let visible_items = item_count
        .min(MINIBAR_SOURCE_MAX_ITEMS)
        .min(minibar_item_capacity(viewport, bar));
    for index in 0..visible_items {
        if let Some(rect) = minibar_item_rect(viewport, bar, index)
            && point_in(rect)
        {
            return Some(MiniBarHit::Item(index));
        }
    }
    if point_in(minibar_panel_rect(viewport)) {
        return Some(MiniBarHit::Body);
    }
    None
}

/// 24×24 Lucide-style "pin-off" glyph used for the unpin affordance.
/// Inline `&'static str` keeps the icon in `.rdata` rather than allocating
/// at construction (§10).
const MINIBAR_UNPIN_PATH: &str = "M2 12L10 4M14 12V20H10V18M22 12L14 4M22 22L2 2";

// -----------------------------------------------------------------------------
// MiniBarController — owns the hibernation triggers for one MiniBar window.
// -----------------------------------------------------------------------------

/// Per-MiniBar hibernation controller. Wraps a [`HibernationGate`]
/// implementation (in production: a `&mut Renderer`; in tests:
/// `RecordingGate`) and exposes the user-facing [`hide`] / [`show`] /
/// [`is_resident`] surface plus the bookkeeping `is_visible` flag.
///
/// Construction-time invariants:
///   * `width`/`height` are the device-pixel size of the MiniBar HWND
///     (`default_size(WindowKind::MiniBar)` = 280×80 at 96 DPI; scaled by
///     the per-window DPI cache before this constructor is called).
///   * The supplied gate STARTS in the resident state (chain present);
///     this matches the post-`Renderer::create` invariant.
///
/// [`hide`]: MiniBarController::hide
/// [`show`]: MiniBarController::show
/// [`is_resident`]: MiniBarController::is_resident
pub struct MiniBarController<G: HibernationGate> {
    gate: G,
    width: u32,
    height: u32,
    is_visible: bool,
}

impl<G: HibernationGate> MiniBarController<G> {
    /// New controller. Caller asserts `gate.is_chain_resident() == true`.
    pub fn new(gate: G, width: u32, height: u32) -> Self {
        Self {
            gate,
            width,
            height,
            is_visible: true,
        }
    }

    /// Hide the MiniBar — issued from `Command::UnpinMinibar` (user clicks
    /// the unpin affordance) or any other path that retracts the pinned
    /// zone (e.g. a `WM_SHOWWINDOW(SW_HIDE)` arriving from the OS, which
    /// the wndproc dispatches to here through the per-window slot).
    ///
    /// The gate's `release_swap_chain` is idempotent, so a double-hide
    /// is a no-op rather than an error. Updates `is_visible` so the next
    /// `show` knows whether work is required.
    pub fn hide(&mut self) {
        if !self.is_visible {
            return;
        }
        self.gate.release_swap_chain();
        self.is_visible = false;
    }

    /// Show the MiniBar — issued from `Command::PinZoneAsMinibar` (user
    /// pins a zone from the context menu) or from a `WM_SHOWWINDOW(SW_SHOW)`
    /// arriving from the OS. Recreates the swap chain at the controller's
    /// recorded `width × height`.
    ///
    /// Returns `Err(SwapChainEnsure)` only when the platform layer fails
    /// to rebuild the chain (e.g. D3D device removed). Idempotent: if
    /// the chain is already resident the call is a cheap branch.
    pub fn show(&mut self) -> Result<(), MiniBarError> {
        if self.is_visible {
            return Ok(());
        }
        self.gate.ensure_swap_chain(self.width, self.height)?;
        self.is_visible = true;
        Ok(())
    }

    /// Whether the controller's user-visible state says "shown". Diverges
    /// from [`is_resident`] only between `hide()` and the first paint
    /// after WM_SHOWWINDOW dispatched the OS-side hide (in practice,
    /// `is_visible == is_resident` on every paint pump tick).
    ///
    /// [`is_resident`]: MiniBarController::is_resident
    #[inline]
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    /// Whether the underlying swap chain is currently resident. Reads
    /// straight through to the gate so `is_resident()` is the truth source
    /// for diagnostics (the smoke test asserts on this directly).
    #[inline]
    pub fn is_resident(&self) -> bool {
        self.gate.is_chain_resident()
    }

    /// Borrow the underlying gate. Tests use this to re-assert the gate's
    /// own bookkeeping (e.g. `RecordingGate::release_count`).
    pub fn gate(&self) -> &G {
        &self.gate
    }
}

// -----------------------------------------------------------------------------
// MiniBarRoster — process-wide cap enforcement (§11 R7).
// -----------------------------------------------------------------------------

/// Tracks every pinned zone id so the user-space `pin` path can refuse
/// the 9th pin at the source rather than silently no-op'ing in the
/// shell registry. `SmallVec<[u64; 8]>` matches the §11 R7 cap so the
/// roster never allocates in steady state.
#[derive(Debug, Default)]
pub struct MiniBarRoster {
    pinned: SmallVec<[u64; MAX_MINIBARS]>,
}

impl MiniBarRoster {
    pub fn new() -> Self {
        Self {
            pinned: SmallVec::new(),
        }
    }

    /// Mark `zone_id` as pinned. Returns `Err(CapReached)` when at the
    /// §11 R7 cap; `Err(AlreadyPinned)` when the id is already in the
    /// roster. On success returns `Ok(remaining)` — the number of free
    /// slots left after this pin (so callers can surface the "X minibars
    /// left" hint without re-reading the roster).
    pub fn pin(&mut self, zone_id: u64) -> Result<usize, MiniBarError> {
        if self.pinned.contains(&zone_id) {
            return Err(MiniBarError::AlreadyPinned);
        }
        if self.pinned.len() >= MAX_MINIBARS {
            return Err(MiniBarError::CapReached);
        }
        self.pinned.push(zone_id);
        Ok(MAX_MINIBARS - self.pinned.len())
    }

    /// Drop `zone_id` from the roster. Returns `Err(NotFound)` when the
    /// id wasn't pinned. Stable order is irrelevant here (the roster is
    /// only used for cap enforcement), so `swap_remove` is the cheaper
    /// path.
    pub fn unpin(&mut self, zone_id: u64) -> Result<(), MiniBarError> {
        match self.pinned.iter().position(|&id| id == zone_id) {
            Some(i) => {
                let _ = self.pinned.swap_remove(i);
                Ok(())
            }
            None => Err(MiniBarError::NotFound),
        }
    }

    /// Currently pinned count.
    pub fn len(&self) -> usize {
        self.pinned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty()
    }

    /// Whether `zone_id` is currently pinned.
    pub fn contains(&self, zone_id: u64) -> bool {
        self.pinned.contains(&zone_id)
    }
}

// -----------------------------------------------------------------------------
// Smoke test — exercises MiniBarController::hide/show against a fake gate
// that records every call. Proves the §11 R5 hibernation wiring without
// requiring a real Renderer / HWND (which need a live D3D device).
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests;
