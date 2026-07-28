//! Render orchestrator.
//!
//! Walks the laid-out tree, dispatches each [`WidgetNode`] to its D2D draw
//! call. All resources are created lazily and cached on the `Renderer`; per
//! frame we only do `BeginDraw`/`Clear`/`Fill*`/`EndDraw`/`Present` (spec §10
//! hot-path discipline — no heap, no `format!`).
//!
//! ### Multi-window state split (T-009 / Wave B)
//!
//! Per Phase 1 / T-009 ruling, resources fall into two tiers:
//!
//! | Tier              | Owner                                    | Cardinality |
//! |-------------------|------------------------------------------|-------------|
//! | Process singleton | `bentodesk-platform` `OnceLock`s        | 1 per process |
//! |                   | — `d2d::factory()` (D2D factory + device)|             |
//! |                   | — `d3d::device()` (D3D11 device + ctx)   |             |
//! |                   | — `dwrite::factory()` (DWrite shared)    |             |
//! |                   | — `dcomp::device()` (DComp v2/v3)        |             |
//! | Per window        | `Renderer` instance (this struct)        | N per process |
//! |                   | — `comp: WindowComp` (DComp visual tree, swap chain) |   |
//! |                   | — `surface: WindowSurface` (D2D RT bound to backbuffer) | |
//! |                   | — `text_format: IDWriteTextFormat`       |             |
//! |                   | — `utf16_scratch`, `base_scale` (per-frame state)    |    |
//!
//! `text_format` lives per-renderer rather than as a singleton because
//! Phase 2 themes let each window kind pick its own system font role while
//! Settings, capsules, and MiniBar can resolve through the shared UI primary.
//! The per-window cost is one COM ref (~1 KB) — well below the 100 MB ceiling
//! even at the §11 R7 max of 8 + 1 windows.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use bentodesk_backend::{
    layout::{BentoZone as SnapshotZone, DesktopSnapshot},
    system::get_memory_usage,
};
use bentodesk_layout::LayoutError;
use bentodesk_platform::{
    Backdrop, PlatformError, WindowKind, backdrop_brush_scale, capture_primary_workarea_blurred,
    d2d::{self, WindowSurface},
    dcomp::WindowComp,
    dwrite, ok, svg,
    svg_cache::SvgCache,
};
use bentodesk_style::{BorderRadius, Color, Lerp, Rect, Shadow, ShadowStack};
use bentodesk_widget::{ImageSource, WidgetNode};
use bentodesk_zone::{Zone, ZoneId, ZoneItem, ZoneItemId};
use smallvec::SmallVec;
use smol_str::SmolStr;
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::HWND as W_HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_POINT_2F, D2D_RECT_F, D2D1_COLOR_F, D2D1_GRADIENT_STOP,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_BITMAP_BRUSH_PROPERTIES,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT, ID2D1Bitmap1, ID2D1BitmapBrush,
    ID2D1LinearGradientBrush, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_FAR, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, IDWriteInlineObject,
    IDWriteTextFormat,
};
use windows::core::Interface;

use crate::animator;
use crate::business::{
    bulk_manager_panel,
    capsule_picker::{self, CapsulePickerHit},
    debug_overlay, highlight_overlay, icon_picker,
    icons::{ALL_ICON_KINDS, IconKind},
    item_card, item_grid, item_icon, minibar, palette_picker, popover,
    rules_wizard::{self, ActionKind, PredicateKind, RunModeChoice, WizardStep},
    search_bar, smart_group_suggestor, stack_tray,
    timeline::{panel as timeline_panel, snapshot_picker},
    tooltip,
};
use crate::dispatcher::PaletteTarget;
use crate::picker_geometry;
use crate::zone_pill_geometry::{self, StackCapsuleLayout, ZonePillLayout};
use crate::{AppState, PanelHeaderButtonKind, WindowState};
use crate::{
    expanded_zone_grid, item_file_rename_geometry, zone_editor_geometry, zone_surface_geometry,
};

// Text-heavy overlays use more than the default body format. Keep the
// DirectWrite format cache tiny, bounded, and inline while still retiring the
// least-recent style instead of constantly replacing the same slot.
#[derive(Debug)]
pub enum RenderError {
    Platform(PlatformError),
    Layout(LayoutError),
    /// Mc-2b — the GPU device was lost (TDR / driver reset / removal). Surfaced
    /// when `WindowComp::present`/`resize` return `PlatformError::DeviceLost`.
    /// The shell chokepoint (Impl C) matches this to drive `recover_device_chain`
    /// plus a per-window rebuild; the renderer self-heals other windows via the
    /// generation check at the top of `render`.
    DeviceLost,
}

impl From<PlatformError> for RenderError {
    fn from(e: PlatformError) -> Self {
        match e {
            // Mc-2b — keep the device-lost signal typed so the `?` on
            // `present()`/`resize()` surfaces a `RenderError::DeviceLost` the
            // shell can match, rather than burying it in `Platform(_)`.
            PlatformError::DeviceLost => RenderError::DeviceLost,
            other => RenderError::Platform(other),
        }
    }
}

impl From<LayoutError> for RenderError {
    fn from(e: LayoutError) -> Self {
        RenderError::Layout(e)
    }
}

impl core::fmt::Display for RenderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RenderError::Platform(e) => write!(f, "render: {e}"),
            RenderError::Layout(e) => write!(f, "render: layout {e:?}"),
            RenderError::DeviceLost => write!(f, "render: device lost"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Per-window renderer owning the D2D surface + DComp tree + brush cache.
///
/// `surface` is `Option` so T-099 can release the D2D bitmap target alongside
/// the DXGI swap chain when the window hibernates. The render hot path
/// short-circuits with `Ok(())` when the surface is absent — paint requests
/// for hidden windows are no-ops until `ensure_swap_chain` rebuilds.
pub struct Renderer {
    pub comp: WindowComp,
    pub surface: Option<WindowSurface>,
    text_format: IDWriteTextFormat,
    text_format_family: SmolStr,
    text_format_size_pt: f32,
    text_format_weight: u16,
    text_format_line_height: f32,
    text_format_cache: SmallVec<[CachedTextFormat; TEXT_FORMAT_CACHE_CAPACITY]>,
    main_region_installed: bool,
    main_region_signature: SmallVec<[DeviceRegionRect; 16]>,
    /// RC-5 Gap A — DWrite ellipsis trimming sign cached against
    /// `text_format`. Lazily created on first `draw_text_no_wrap` call and
    /// invalidated whenever `ensure_text_format_for_active_theme` swaps the
    /// underlying format so the `…` glyph stays in sync with theme typography.
    /// Spec §10 — one COM allocation per format recreate, zero per frame.
    ellipsis_sign: Option<IDWriteInlineObject>,
    /// Stack bloom petal names use the 11.5px/600 two-line title token.
    /// Cache its trimming sign separately so the wrapped final line uses the
    /// same glyph metrics as the petal label format.
    bloom_petal_ellipsis_sign: Option<IDWriteInlineObject>,
    /// M1i fidelity (2026-05-29) — lazily-created monospace text format for the
    /// §2 desktop-source `.desktop-source-card__path` line (Tauri
    /// `font-family: ui-monospace, Consolas, monospace`, `font-size: 11px`).
    /// Cached per (size_pt) so the path run uses fixed-pitch glyphs instead of
    /// the proportional YaHei UI body font. One COM allocation per recreate,
    /// zero per frame (spec §10). Paired with [`Self::monospace_ellipsis_sign`]
    /// so the path can character-trim with an inline `…` when it overflows.
    monospace_format: Option<CachedTextFormat>,
    /// M1i fidelity — `…` trimming sign tied to [`Self::monospace_format`].
    monospace_ellipsis_sign: Option<IDWriteInlineObject>,
    /// G5 (2026-06-01) — cached DASHED stroke style for the collapsed
    /// `minimal`-shape capsule border (`BentoZone.css:92-99` `1px dashed`).
    /// Built from the device-INDEPENDENT D2D factory so it survives device
    /// rebuilds; one COM allocation per process, zero per frame (§10).
    dashed_stroke_style: Option<ID2D1StrokeStyle>,
    /// V21-C2 -- single-slot D2D linear-gradient brush cache. Rebuilt only
    /// when the two RGBA stops change; draw calls mutate only start/end points.
    linear_gradient_brush: Option<CachedLinearGradientBrush>,
    /// Ellipsis sign tied to the stable collapsed-pill title role. Size changes
    /// keep 13-DIP text and trim the line instead of shrinking it toward 8px.
    /// Invalidated with the active theme font and lazily rebuilt once.
    pill_title_ellipsis_sign: Option<IDWriteInlineObject>,
    /// V21-C6 (2026-06-22) — separate shrink memo for Tauri `StackCapsule`
    /// titles. Stack capsule text uses 13px / 600 / no tracking, so sharing the
    /// ordinary pill cache would either mis-key on typography or thrash when a
    /// stack capsule and ordinary capsule are both visible.
    stack_capsule_title_shrink: Option<(u64, f32)>,
    pub width: u32,
    pub height: u32,
    /// Reusable UTF-16 scratch buffer (spec §10).
    utf16_scratch: SmallVec<[u16; 256]>,
    /// M7 (2026-06-01) — reusable scratch for the §10 Encryption card's masked
    /// passphrase string ('•' × draft-char-count, + an optional caret glyph).
    /// Cleared (never freed) each paint so the mask render allocates nothing
    /// per frame (spec §10). NEVER holds the literal passphrase.
    mask_scratch: String,
    /// Phase 2.3.1b — scale factor applied to D2D's world transform for the
    /// current frame. Equal to `dpi / 96` (1.0 at 96 DPI). Stashed on the
    /// renderer so per-glyph SVG transforms can compose against it instead
    /// of clobbering the base scale with `SetTransform(identity)`.
    /// Updated once per `render()` call.
    base_scale: f32,
    /// V21-A — optional current logical transform for a grouped surface
    /// animation. SVG/text helpers restore to this matrix while active so a
    /// nested icon draw cannot accidentally cancel the Settings scale-in.
    logical_transform_override: Option<Matrix3x2>,
    /// One-shot scale-in clock for compact auxiliary surfaces. The shell
    /// restarts it only when a hidden aux HWND is shown.
    auxiliary_open_started_ms: Option<u32>,
    /// D2D bitmap cache keyed by backend icon hash. This is the runtime bridge
    /// that makes `LoadIcon` visible in the selected-stack executable instead
    /// of falling back to emoji placeholders forever.
    icon_bitmaps: HashMap<String, ID2D1Bitmap1>,
    /// Hashes that failed cache lookup or WIC decode. Avoids retrying disk/WIC
    /// work every frame while preserving fallback rendering.
    icon_bitmap_failures: HashSet<String>,
    /// D2D bitmap cache keyed by file path for retained Image widgets.
    image_file_bitmaps: HashMap<String, ID2D1Bitmap1>,
    /// File paths that failed read or WIC decode during this renderer lifetime.
    image_file_failures: HashSet<String>,
    /// Full SVG document geometry cache for source Tauri zone icons.
    svg_cache: SvgCache,
    /// Monotonic clock base for DebugOverlay RSS sampling. Stored on the
    /// renderer so the HUD never depends on wall-clock time changes.
    debug_overlay_started_at: Instant,
    /// Mc-2b — the HWND this renderer paints into. Stashed at `create` time so
    /// `rebuild_after_device_loss` can re-run `WindowComp::create(hwnd, ..)`
    /// against a freshly-recovered device chain without the shell threading the
    /// handle back through.
    hwnd: W_HWND,
    /// Mc-2b — the device generation observed when this renderer's
    /// device-derived COM was last built. The paint entry compares this against
    /// `platform::device_generation()`; a mismatch means the chain was rebuilt
    /// by another window's recovery, so this renderer self-heals before drawing.
    device_gen: u64,
    /// Frosted-backdrop (real-acrylic) cached snapshot — the baked, blurred
    /// primary work-area bitmap behind every Main-overlay zone surface. `None`
    /// = no frost (not yet captured, capture failed → degrade to flat tint, or
    /// `FROSTED_BACKDROP` disabled). Rebuilt only on `backdrop_dirty` (spec §10:
    /// no per-frame capture); Main-overlay-only (other windows never touch it).
    backdrop: Option<Backdrop>,
    /// Frosted-backdrop refresh flag. Set `true` at `create` (first-paint
    /// capture), and by `mark_backdrop_dirty` on display / wallpaper / show
    /// events. The next Main-overlay `render()` re-captures, then clears it.
    backdrop_dirty: bool,
    /// Saturation factor used by the current cached `backdrop`. Theme polarity
    /// flips (dark ↔ light) re-use the same bitmap slot but must re-bake it so
    /// the CSS `--blur-zen` saturation token follows the active theme.
    backdrop_saturation: f32,
    /// Frosted-backdrop per-frame bitmap brush built ONCE from `backdrop`
    /// (spec §10 hot path). Cleared to `None` at the START of every frame so a
    /// non-Main frame or a `None` backdrop never reuses a stale brush; rebuilt
    /// for the Main overlay after `BeginDraw`. `fill_frosted_rect` reads it.
    backdrop_brush: Option<ID2D1BitmapBrush>,
}

mod capsule_bulk;
mod context_minibar;
mod device;
mod item_cards;
mod item_pickers;
mod keybindings_about;
mod overlays;
mod rules_surface;
mod search_suggestor;
mod settings;
mod settings_appearance;
mod settings_backup;
mod settings_context;
mod settings_encryption;
mod settings_general_paths;
mod settings_performance_startup;
mod settings_plugins;
mod settings_stealth;
mod settings_updater;
mod shape_primitives;
mod stack_surfaces;
mod svg_drawing;
mod text_core;
mod text_titles;
mod timeline_snapshot;
mod zone_bloom;
mod zone_chrome;
mod zone_editor;
mod zones;

mod drag_geometry;
mod localization;
mod model;
mod preview_geometry;
mod region_geometry;
mod stack_visuals;
mod text_helpers;

use drag_geometry::*;
use localization::*;
use model::*;
use preview_geometry::*;
use region_geometry::*;
use settings_context::*;
use stack_visuals::*;
pub use text_helpers::settings_caret_on;
use text_helpers::*;

#[cfg(test)]
mod tests;
