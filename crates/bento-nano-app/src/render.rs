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
//! | Process singleton | `bento-nano-platform` `OnceLock`s        | 1 per process |
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
//! Phase 2 themes will let each window kind pick its own font (e.g. Settings
//! uses Segoe UI Variable while MiniBar stays on Microsoft YaHei UI). The
//! per-window cost is one COM ref (~1 KB) — well below the 100 MB ceiling
//! even at the §11 R7 max of 8 + 1 windows.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use bento_nano_backend::{
    layout::{BentoZone as SnapshotZone, DesktopSnapshot},
    system::get_memory_usage,
};
use bento_nano_layout::LayoutError;
use bento_nano_platform::{
    PlatformError, WindowKind,
    d2d::{self, WindowSurface},
    dcomp::WindowComp,
    dwrite, ok, svg,
    svg_cache::SvgCache,
};
use bento_nano_style::{BorderRadius, Color, Rect};
use bento_nano_widget::{ImageSource, WidgetNode};
use bento_nano_zone::{Zone, ZoneId, ZoneItem, ZoneItemId};
use smallvec::SmallVec;
use smol_str::SmolStr;
use windows::Win32::Foundation::HWND as W_HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D_POINT_2F, D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1Bitmap1,
    ID2D1RenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{IDWriteInlineObject, IDWriteTextFormat};
use windows::core::Interface;

use crate::business::{
    bulk_manager_panel, capsule_picker, debug_overlay, highlight_overlay, icon_picker,
    icons::{ALL_ICON_KINDS, IconKind},
    item_card, item_grid, item_icon, minibar, palette_picker,
    rules_wizard::{self, ActionKind, PredicateKind, RunModeChoice, WizardStep},
    search_bar, smart_group_suggestor, stack_tray,
    timeline::{panel as timeline_panel, snapshot_picker},
    tooltip,
};
use crate::animator;
use crate::dispatcher::PaletteTarget;
use crate::picker_geometry;
use crate::zone_pill_geometry::{self, ZonePillLayout};
use crate::{AppState, WindowState};
use crate::{
    expanded_zone_grid, item_file_rename_geometry, zone_editor_geometry, zone_surface_geometry,
};

const TEXT_FORMAT_CACHE_CAPACITY: usize = 8;
const IMAGE_WIDGET_MAX_BYTES: usize = 32 * 1024 * 1024;

/// M2③ (05-31, 1:1) — thickness of the expanded-panel top accent edge in
/// logical px. Matches Tauri `.bento-zone--expanded { border-top: 2px solid
/// var(--zone-accent, transparent) }` (BentoZone.css:114). Const-only so the
/// per-frame zone draw stays allocation-free (§10).
const PANEL_ACCENT_EDGE_THICKNESS_PX: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveItemDragVisual {
    zone_id: ZoneId,
    item_id: ZoneItemId,
    last_x: f32,
    last_y: f32,
}

#[derive(Clone)]
struct CachedTextFormat {
    family: SmolStr,
    size_pt: f32,
    weight: u16,
    line_height: f32,
    format: IDWriteTextFormat,
}

/// Render-pipeline error variants.
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
    /// RC-5 Gap A — DWrite ellipsis trimming sign cached against
    /// `text_format`. Lazily created on first `draw_text_no_wrap` call and
    /// invalidated whenever `ensure_text_format_for_active_theme` swaps the
    /// underlying format so the `…` glyph stays in sync with theme typography.
    /// Spec §10 — one COM allocation per format recreate, zero per frame.
    ellipsis_sign: Option<IDWriteInlineObject>,
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
    pub width: u32,
    pub height: u32,
    /// Reusable UTF-16 scratch buffer (spec §10).
    utf16_scratch: SmallVec<[u16; 256]>,
    /// Phase 2.3.1b — scale factor applied to D2D's world transform for the
    /// current frame. Equal to `dpi / 96` (1.0 at 96 DPI). Stashed on the
    /// renderer so per-glyph SVG transforms can compose against it instead
    /// of clobbering the base scale with `SetTransform(identity)`.
    /// Updated once per `render()` call.
    base_scale: f32,
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
}

impl Renderer {
    pub fn create(hwnd: W_HWND, width: u32, height: u32) -> Result<Self, RenderError> {
        let comp = WindowComp::create(hwnd, width, height)?;
        // `WindowComp::create` always installs a swap chain; the only path
        // that nulls it is T-099 hibernation, which can't run during
        // construction.
        let swap = comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::create: swap_chain missing immediately after WindowComp::create",
            ),
        ))?;
        let surface = WindowSurface::create(swap)?;
        // #19-B (2026-05-31) — resolve the UI default against the installed
        // system fonts. On a normal Windows "Microsoft YaHei UI" is present so
        // this returns the same literal as before (Q2 pixel-1:1); on a stripped
        // SKU it falls back through Segoe UI / Tahoma. ("MS Shell Dlg 2" is a
        // GDI alias DWrite's FindFamilyName cannot resolve — it would always
        // probe-miss — and the resolver's universal tail is already Tahoma, so
        // it is omitted as dead weight.)
        let ui_family: &'static str = dwrite::resolve_default_family(
            dwrite::FontRole::Ui,
            &["Microsoft YaHei UI", "Segoe UI", "Tahoma"],
        );
        let text_format = dwrite::text_format_from_family_name_with_metrics(
            ui_family,
            16.0,
            400,
            1.4,
            dwrite::locale_zh_cn(),
        )?;
        Ok(Self {
            comp,
            surface: Some(surface),
            text_format,
            text_format_family: SmolStr::new_static(ui_family),
            text_format_size_pt: 16.0,
            text_format_weight: 400,
            text_format_line_height: 1.4,
            text_format_cache: SmallVec::new(),
            ellipsis_sign: None,
            monospace_format: None,
            monospace_ellipsis_sign: None,
            width,
            height,
            utf16_scratch: SmallVec::new(),
            // 1.0 = 96 DPI baseline. `render()` overwrites this each frame
            // from `WindowState.dpi` before any draw call observes it.
            base_scale: 1.0,
            icon_bitmaps: HashMap::new(),
            icon_bitmap_failures: HashSet::new(),
            image_file_bitmaps: HashMap::new(),
            image_file_failures: HashSet::new(),
            svg_cache: SvgCache::default(),
            debug_overlay_started_at: Instant::now(),
            hwnd,
            device_gen: bento_nano_platform::device_generation(),
        })
    }

    /// Re-create the swap chain backbuffer surface after a resize.
    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), RenderError> {
        if let Some(s) = self.surface.as_mut() {
            s.release_target();
        }
        self.comp.resize(w, h)?;
        // When the chain was hibernated, ensure_chain has to be the call site
        // that recreates it — but we still re-bind the surface here so a
        // resize between hibernate-and-show keeps width/height in sync.
        if let Some(swap) = self.comp.swap_chain.as_ref() {
            self.surface = Some(WindowSurface::create(swap)?);
        } else {
            self.surface = None;
        }
        self.width = w;
        self.height = h;
        Ok(())
    }

    /// T-099 — drop the per-window backbuffer (largest per-window allocation,
    /// ~1.2 MB at 480×320×4×2). Surface and swap chain go; visual tree +
    /// DComp target stay so a subsequent `ensure_swap_chain` rebinds without
    /// re-creating the composition. Idempotent: a second call is a no-op.
    pub fn release_swap_chain(&mut self) {
        if let Some(s) = self.surface.as_mut() {
            s.release_target();
        }
        self.surface = None;
        self.comp.release_chain();
    }

    /// T-099 — recreate the backbuffer + D2D surface after `release_swap_chain`.
    /// Idempotent: returns `Ok(())` immediately if already resident. Called by
    /// the wndproc paint guard before each paint to lift hibernation lazily.
    pub fn ensure_swap_chain(&mut self, w: u32, h: u32) -> Result<(), RenderError> {
        if self.surface.is_some() && self.comp.swap_chain.is_some() {
            return Ok(());
        }
        self.comp.ensure_chain(w.max(1), h.max(1))?;
        let swap = self.comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::ensure_swap_chain: chain still missing after ensure_chain",
            ),
        ))?;
        self.surface = Some(WindowSurface::create(swap)?);
        self.width = w;
        self.height = h;
        Ok(())
    }

    /// Mc-2b — rebuild this window's device-derived COM after a device-lost
    /// event. PRECONDITION: the shell (Impl C chokepoint) has ALREADY called
    /// `platform::recover_device_chain()`, so the process-singleton D3D/D2D/
    /// DComp devices are fresh; this method only rebuilds the per-window objects
    /// that were bound to the dead device. If any step errors it propagates —
    /// the shell's retry cap (Impl C) handles repeated failure.
    pub fn rebuild_after_device_loss(&mut self) -> Result<(), RenderError> {
        // Drop the old D2D context + bitmap target first; both are bound to the
        // dead device and would keep it alive.
        self.surface = None;
        // Rebuild the composition (swap chain + DComp target + root visual) on
        // the recovered device. Replacing `self.comp` drops every old object.
        self.comp = WindowComp::create(self.hwnd, self.width, self.height)?;
        // Mirror `create`: bind a fresh D2D surface to the new backbuffer.
        let swap = self.comp.swap_chain.as_ref().ok_or(RenderError::Platform(
            bento_nano_platform::PlatformError::Init(
                "Renderer::rebuild_after_device_loss: swap_chain missing immediately after WindowComp::create",
            ),
        ))?;
        self.surface = Some(WindowSurface::create(swap)?);
        // Clear device-derived caches: these bitmaps/geometries were created on
        // the now-dead D2D device/factory and must be re-decoded/re-built on the
        // recovered ones. Failure entries also reset so previously-failing icons
        // get one fresh attempt against the new device.
        self.icon_bitmaps.clear();
        self.icon_bitmap_failures.clear();
        self.image_file_bitmaps.clear();
        self.image_file_failures.clear();
        self.svg_cache.clear();
        // KEEP DWrite-derived state untouched: `text_format`,
        // `text_format_cache`, `ellipsis_sign`, `monospace_format`,
        // `monospace_ellipsis_sign`. DWrite is GPU-INDEPENDENT (design §B / A2),
        // so these survive a device loss and never need rebuilding here.
        self.device_gen = bento_nano_platform::device_generation();
        Ok(())
    }

    /// Whether this renderer currently owns a swap chain. Diagnostics +
    /// the wndproc paint guard read this to decide if a paint should
    /// trigger `ensure_swap_chain` first.
    #[inline]
    pub fn is_resident(&self) -> bool {
        self.surface.is_some() && self.comp.swap_chain.is_some()
    }

    /// Run one frame: layout + draw + present. `win` carries the per-HWND
    /// `LayoutEngine` (cache lives there — Ruling 5 / C3).
    ///
    /// Phase 2.3.1b — `self.width / self.height` are **device pixels** (the
    /// swap chain backbuffer dimensions reported by `WM_SIZE` /
    /// `GetClientRect`). The layout engine + zone collection live in
    /// **logical** units (DIPs), so we divide by `dpi/96` once to obtain the
    /// logical viewport. A single `SetTransform(Scale)` after `BeginDraw`
    /// then projects every logical coordinate onto the right device pixel
    /// without per-call multiplication.
    pub fn render(
        &mut self,
        app: &mut AppState,
        win: &mut WindowState,
        kind: WindowKind,
    ) -> Result<(), RenderError> {
        // Mc-2b — generation self-heal. When another window hit DeviceLost and
        // the shell bumped the generation via `recover_device_chain`, this
        // renderer's device-derived COM is stale; rebuild it on this paint
        // before any draw call touches the dead device. One atomic load per
        // paint entry (§10): `present()` is reached from this single function,
        // so one check here covers both present sites below. The rebuild path
        // is cold (only runs on the first paint after a device loss).
        if renderer_is_stale(self.device_gen, bento_nano_platform::device_generation()) {
            self.rebuild_after_device_loss()?;
        }
        // §10 hot-path: read once, no allocation.
        let frame_started_at = Instant::now();
        let dpi = win.dpi.get();
        let scale = bento_nano_style::dpi::scale_factor(dpi);
        let device_size = bento_nano_style::Size {
            width: self.width as f32,
            height: self.height as f32,
        };
        // Phase 2.3.1b — viewport flipped from device-pixel to logical-DIP.
        // At 96 DPI the conversion is identity (regression-safe); at 192
        // DPI a 960×640 backbuffer becomes a 480×320 logical viewport so
        // the same layout source produces the same logical rects.
        app.viewport = bento_nano_style::dpi::device_size_to_logical(device_size, dpi);
        self.ensure_text_format_for_active_theme(app)?;
        // Phase 2.1 / Ruling A + Q2 — first-paint zone load.
        //
        // Error-class routing:
        //   Ok(list)                  → adopt the list.
        //   Err(Storage(_))           → structural corruption (bad magic /
        //                               version mismatch / truncated). Rename
        //                               the file so the user can recover it,
        //                               start empty.
        //   Err(StorageIo { kind: NotFound, .. })
        //                             → handled inside `read_zones` itself
        //                               (returns Ok(empty)); never reaches
        //                               this arm.
        //   Err(StorageIo { .. })     → access issue (permission denied,
        //                               sharing violation). DON'T rename —
        //                               the file is probably fine, we just
        //                               can't open it now. Start empty.
        //
        // Either branch flips `loaded` so the paint hot path never retries.
        if !win.loaded.get() {
            if !app.zones_path.as_os_str().is_empty() {
                match bento_nano_platform::storage::read_zones(&app.zones_path) {
                    Ok(loaded) => {
                        app.zones = loaded;
                    }
                    Err(bento_nano_platform::PlatformError::Storage(_)) => {
                        let _ = bento_nano_platform::storage::quarantine_corrupt(&app.zones_path);
                    }
                    Err(_) => {
                        // IO / permission / other — leave the file in place.
                    }
                }
            }
            win.loaded.set(true);
        }
        // Phase 2.3.1b — record `base_scale` for the frame so SVG draw paths
        // can compose against it instead of resetting to identity.
        self.base_scale = scale;

        // T-099 — paint guard. When the swap chain is hibernated, return
        // `Ok(())`. The wndproc's WM_PAINT arm calls `ensure_swap_chain`
        // before paint when a window becomes visible again, so this only
        // fires for genuine "skip this frame" cases (e.g. paint queued
        // between hibernate and the next show event).
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        let ctx = &surface.ctx;

        // SAFETY: surface valid (just unwrapped); D2D draw sequence
        //         BeginDraw → ... → EndDraw, no re-entry between calls.
        unsafe {
            ctx.BeginDraw();
            let clear = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            ctx.Clear(Some(&clear));
            // Phase 2.3.1b — single SetTransform projects the entire logical
            // coordinate space onto device pixels. Every fill / draw call
            // below this point uses logical units; D2D multiplies by `scale`
            // automatically. SVG paths re-establish this base scale via
            // `base_scale_matrix()` because their per-glyph transforms also
            // need the projection.
            let base = base_scale_matrix(scale);
            ctx.SetTransform(&base);
        }

        let rendered_aux_window = match kind {
            WindowKind::ZoneEditor => {
                self.draw_zone_editor_window(app)?;
                true
            }
            WindowKind::ItemFileRename => {
                self.draw_item_file_rename_window(app)?;
                true
            }
            WindowKind::IconPicker => {
                self.draw_icon_picker_window(app)?;
                true
            }
            WindowKind::PalettePicker => {
                self.draw_palette_picker_window(app)?;
                true
            }
            WindowKind::CapsulePicker => {
                self.draw_capsule_picker_window(app)?;
                true
            }
            WindowKind::RulesWizard => {
                self.draw_rules_wizard_window(app)?;
                true
            }
            WindowKind::BulkManager => {
                self.draw_bulk_manager_window(app)?;
                true
            }
            WindowKind::Timeline => {
                self.draw_timeline_window(app)?;
                true
            }
            WindowKind::SnapshotPicker => {
                self.draw_snapshot_picker_window(app)?;
                true
            }
            WindowKind::Suggestor => {
                self.draw_suggestor_window(app)?;
                true
            }
            WindowKind::Search => {
                self.draw_search_window(app)?;
                true
            }
            WindowKind::MiniBar => {
                self.draw_minibar_window(app)?;
                true
            }
            WindowKind::Tooltip => {
                self.draw_tooltip_window(app)?;
                true
            }
            WindowKind::About => {
                self.draw_about_panel(app)?;
                true
            }
            WindowKind::Settings => {
                self.draw_settings_window(app)?;
                true
            }
            _ => false,
        };
        if rendered_aux_window {
            // M6c — scanline post-pass over the aux surface (terminal theme
            // only; no-op otherwise). Tauri's `data-theme-effect` `::after` is
            // a per-document `position:fixed; inset:0` overlay, so each nano
            // HWND paints it over its own client area just before EndDraw.
            self.draw_effect_overlay(app)?;
            let end_ctx = self.ctx()?;
            // SAFETY: surface valid (guarded at the top of render); this
            // closes the auxiliary frame started by BeginDraw above.
            let end = unsafe { end_ctx.EndDraw(None, None) };
            ok("EndDraw", end)?;
            self.comp.present()?;
            return Ok(());
        }

        // Collect (id, rect) pairs into a stack-inlined buffer so the layout
        // result borrow doesn't outlive the dispatch loop (which mutably
        // borrows `self` via `draw_node`).
        let mut ids: SmallVec<[(bento_nano_tree::NodeId, bento_nano_style::Rect); 32]> =
            SmallVec::new();
        {
            let result = win.layout.layout(&app.tree, app.viewport)?;
            for (id, rect) in result.iter() {
                ids.push((*id, *rect));
            }
        }

        for (id, rect) in ids.iter() {
            let node = match app.tree.get(*id) {
                Ok(n) => n,
                Err(_) => continue,
            };
            self.draw_node(node, *rect)?;
        }
        // α5 (S2, 2026-05-24): the prior unconditional `draw_theme_base_accent`
        // call painted a 4-DIP accent strip across the full top edge of the
        // Main HWND on every frame. The Tauri 1.2.4 baseline paints no such
        // strip (grep on bentodesk@6a3b283 returns zero `theme-base` /
        // `base-accent` consumers). On the desktop overlay the strip read as
        // an ugly blue border riding above all foreground apps. The state
        // field + helper stay alive for zone-accent fallback (consumed at
        // :1235/1283/1303/1391 below) and for the picker pop-up that lets
        // users pick the base accent; only the Main-HWND leak is removed.

        // Phase 2 — zones live outside the widget tree (they're a domain
        // collection, not a tree-mounted card). Render after the tree so
        // they paint on top of the toolbar card; geometry comes straight
        // from `Zone.x/y/w/h` (DIPs).
        self.draw_zones(app)?;
        self.draw_highlight_overlay(app)?;
        if !app.settings_open.get() && !app.about_open.get() {
            self.draw_stack_tray_overlay(app)?;
        }

        // Wave K1b — Settings and About each own a dedicated aux HWND (the
        // `WindowKind::Settings` / `WindowKind::About` arms above route to
        // `draw_settings_window` / `draw_about_panel`). Painting the modal a
        // second time on the Main HWND duplicates the panel chrome onto the
        // overlay (two scrims, two cards) which becomes visible after H4
        // raised both surfaces to `WS_EX_TOPMOST`. Skip the legacy Main-side
        // fallback here.
        self.poll_debug_overlay_rss(app);
        self.draw_debug_overlay(app)?;

        // M6c — scanline post-pass over the main desktop surface (terminal
        // theme only; no-op otherwise), AFTER all zones / overlays / debug so
        // the green bands ride on top of everything (`z-index:9999`).
        self.draw_effect_overlay(app)?;

        // SAFETY: surface valid (guarded at the top of this fn); EndDraw
        //         signals the end of this frame's work.
        let end_ctx = self.ctx()?;
        let end = unsafe { end_ctx.EndDraw(None, None) };
        ok("EndDraw", end)?;
        self.comp.present()?;
        self.record_debug_overlay_frame(app, kind, frame_started_at);
        Ok(())
    }

    fn debug_overlay_elapsed_ms(&self) -> u32 {
        u32::try_from(self.debug_overlay_started_at.elapsed().as_millis()).unwrap_or(u32::MAX)
    }

    fn ensure_text_format_for_active_theme(&mut self, app: &AppState) -> Result<(), RenderError> {
        let typography = app.active_theme_typography();
        let family = typography.font_family;
        let size_pt = typography.sizes.md.max(1.0);
        let weight = dwrite::normalize_font_weight(typography.weights.normal);
        let line_height = dwrite::normalize_line_height(typography.line_heights.normal);
        if self.text_format_family == family
            && (self.text_format_size_pt - size_pt).abs() < f32::EPSILON
            && self.text_format_weight == weight
            && (self.text_format_line_height - line_height).abs() < f32::EPSILON
        {
            return Ok(());
        }
        self.text_format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            weight,
            line_height,
            dwrite::locale_zh_cn(),
        )?;
        self.text_format_family = family;
        self.text_format_size_pt = size_pt;
        self.text_format_weight = weight;
        self.text_format_line_height = line_height;
        self.text_format_cache.clear();
        // RC-5 Gap A — the ellipsis sign captures the *previous* format's
        // typography (size/weight/family); drop it so the next no-wrap
        // draw lazily re-creates a sign against the new format. One COM
        // allocation per theme/font swap, none per frame.
        self.ellipsis_sign = None;
        Ok(())
    }

    fn poll_debug_overlay_rss(&self, app: &AppState) {
        let now_ms = self.debug_overlay_elapsed_ms();
        let should_poll = {
            let state = app.debug_overlay.borrow();
            state.visible && state.rss_sample_due(now_ms)
        };
        if !should_poll {
            return;
        }
        let memory = get_memory_usage();
        let rss_mb = (memory.working_set_bytes / 1024) as f32 / 1024.0;
        let _recorded = app
            .debug_overlay
            .borrow_mut()
            .record_rss_if_due(now_ms, rss_mb);
    }

    fn record_debug_overlay_frame(
        &self,
        app: &AppState,
        kind: WindowKind,
        frame_started_at: Instant,
    ) {
        if kind != WindowKind::Main {
            return;
        }
        let elapsed_us = u32::try_from(frame_started_at.elapsed().as_micros()).unwrap_or(u32::MAX);
        app.debug_overlay.borrow_mut().record_frame(elapsed_us);
    }

    fn draw_debug_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        let (fps, rss_mb, frame_us) = {
            let state = app.debug_overlay.borrow();
            if !state.visible {
                return Ok(());
            }
            (state.fps(), state.last_rss_mb, state.last_frame_us)
        };
        let chrome = debug_overlay::DebugOverlayChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_spacing(),
            app.active_theme_shadow(),
        );
        let panel = Rect {
            x: (app.viewport.width - debug_overlay::OVERLAY_WIDTH - debug_overlay::EDGE_MARGIN)
                .max(debug_overlay::EDGE_MARGIN),
            y: debug_overlay::EDGE_MARGIN,
            width: debug_overlay::OVERLAY_WIDTH,
            height: debug_overlay::OVERLAY_HEIGHT,
        };
        let shadow = debug_overlay::panel_shadow_rect(panel, chrome.shadow);
        self.fill_rounded_rect(shadow, chrome.shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel, chrome.panel_radius)?;
        let text_width = panel.width - chrome.text_inset_x * 2.0;
        self.draw_text(
            "Debug Overlay",
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.title_top,
                width: text_width,
                height: chrome.title_height,
            },
            chrome.title,
        )?;
        let fps_line = format!("FPS: {fps:>3}");
        let rss_line = format!("RSS: {rss_mb:>4.1} MB");
        let frame_line = format!("Frame: {:>5.2} ms", frame_us as f32 / 1000.0);
        self.draw_text(
            &fps_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.body,
        )?;
        self.draw_text(
            &rss_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top + chrome.metric_row_gap,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.body,
        )?;
        self.draw_text(
            &frame_line,
            Rect {
                x: panel.x + chrome.text_inset_x,
                y: panel.y + chrome.metric_first_top + chrome.metric_row_gap * 2.0,
                width: text_width,
                height: chrome.metric_row_height,
            },
            chrome.muted,
        )
    }

    fn draw_highlight_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        let overlay = app.highlight_overlay.borrow();
        if !overlay.has_targets() {
            return Ok(());
        }
        // Wave E: Tauri SSoT tokens for highlight overlay accents.
        // M6a — re-skin from the live theme palette (bound once per fn, §10).
        let pal = app.active_theme_tauri();
        let fill = highlight_overlay::fill_color_from_tauri_palette(pal);
        let outline = highlight_overlay::outline_color_from_tauri_palette(pal);
        let radius =
            highlight_overlay::target_radius_from_tauri_tokens(app.active_theme_radius_tauri());
        for target in overlay.targets().iter().copied() {
            let paint = highlight_overlay::paint_rect(target);
            if paint.width <= 0.0 || paint.height <= 0.0 {
                continue;
            }
            if overlay.show_outline() {
                self.fill_rounded_rect(paint, outline, radius)?;
                let inner = inset_rect(paint, highlight_overlay::OUTLINE_WIDTH_PX);
                self.fill_rounded_rect(inner, fill, radius)?;
            } else {
                self.fill_rounded_rect(paint, fill, radius)?;
            }
        }
        if !overlay.pulses().is_empty() {
            let phase = overlay.current_pulse_phase();
            let halo = highlight_overlay::pulse_halo_color_from_tauri_palette(pal, phase);
            let core = highlight_overlay::pulse_core_color_from_tauri_palette(pal);
            for target in overlay.pulses() {
                let halo_rect = highlight_overlay::pulse_halo_rect(target, phase);
                if halo_rect.width > 0.0 && halo_rect.height > 0.0 {
                    self.fill_rounded_rect(
                        halo_rect,
                        halo,
                        BorderRadius::all(halo_rect.width * 0.5),
                    )?;
                }
                let core_rect = highlight_overlay::pulse_core_rect(target);
                if core_rect.width > 0.0 && core_rect.height > 0.0 {
                    self.fill_rounded_rect(
                        core_rect,
                        core,
                        BorderRadius::all(core_rect.width * 0.5),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn draw_node(
        &mut self,
        node: &WidgetNode,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        match node {
            WidgetNode::Container(c) => {
                self.fill_rounded_rect(rect, c.background, c.radius)?;
            }
            WidgetNode::Button(b) => {
                self.fill_rounded_rect(rect, b.background, b.radius)?;
                if !b.label.is_empty() {
                    self.draw_text(&b.label, rect, b.label_color)?;
                }
            }
            WidgetNode::Text(t) => {
                self.draw_text_with_style(
                    t.resolved(),
                    rect,
                    t.color,
                    t.font_size_pt,
                    t.font_weight,
                    t.line_height,
                )?;
            }
            WidgetNode::Image(img) => {
                if let ImageSource::SvgPath(path) = &img.source {
                    if !path.is_empty() {
                        self.draw_svg(path.as_str(), rect, img.tint)?;
                    }
                } else if let ImageSource::File(path) = &img.source {
                    self.draw_image_file(path.as_str(), rect)?;
                }
            }
            WidgetNode::BentoCard(card) => {
                // Shadow rendering hooks into D2D's shadow effect in PHASE_2;
                // for now we draw the rounded fill so the card geometry is
                // visible in the spike. Spec §17 — shadow is non-lever
                // visual polish and stays out of Phase 1.2's binary budget.
                self.fill_rounded_rect(rect, card.background, card.border_radius)?;
            }
            WidgetNode::Toolbar(_) => {
                // Toolbar is a flex container with no own visual — children
                // are dispatched by the outer iter loop. Nothing to draw
                // here, intentionally.
            }
            WidgetNode::IconButton(ib) => {
                // Hover background — interpolate alpha by hover_progress.
                let p = ib.hover_progress();
                if p > 0.0 {
                    let bg = bento_nano_style::Color {
                        a: ib.hover_background.a * p,
                        ..ib.hover_background
                    };
                    self.fill_rounded_rect(rect, bg, ib.hover_radius)?;
                }
                // SVG glyph — `svg_path` is a 24×24 viewbox path. `draw_svg`
                // applies scale-to-fit using the icon's source viewbox.
                if !ib.svg_path.is_empty() {
                    self.draw_svg_fit(ib.svg_path, rect, ib.tint, 24.0)?;
                }
            }
            WidgetNode::ScrollContainer(_) => {
                // Container with no own visual — content clipping happens
                // when the layout engine grows clip-rect support
                // (PHASE_2). Children are dispatched by the outer iter
                // loop, so the static frame is correct today.
            }
            WidgetNode::Checkbox(c) => {
                let p = c.fill_progress();
                let bg = bento_nano_style::Color {
                    r: c.box_color.r + (c.box_color_checked.r - c.box_color.r) * p,
                    g: c.box_color.g + (c.box_color_checked.g - c.box_color.g) * p,
                    b: c.box_color.b + (c.box_color_checked.b - c.box_color.b) * p,
                    a: c.box_color.a + (c.box_color_checked.a - c.box_color.a) * p,
                };
                self.fill_rounded_rect(rect, bg, c.radius)?;
            }
            WidgetNode::Toggle(t) => {
                let p = t.thumb_anim.current();
                let bg = bento_nano_style::Color {
                    r: t.track_off.r + (t.track_on.r - t.track_off.r) * p,
                    g: t.track_off.g + (t.track_on.g - t.track_off.g) * p,
                    b: t.track_off.b + (t.track_on.b - t.track_off.b) * p,
                    a: t.track_off.a + (t.track_on.a - t.track_off.a) * p,
                };
                self.fill_rounded_rect(rect, bg, t.track_radius)?;
                let thumb_x = rect.x
                    + bento_nano_widget::toggle::THUMB_INSET_PX
                    + (rect.width
                        - bento_nano_widget::toggle::THUMB_DIAMETER_PX
                        - 2.0 * bento_nano_widget::toggle::THUMB_INSET_PX)
                        * p;
                let thumb_rect = bento_nano_style::Rect {
                    x: thumb_x,
                    y: rect.y + bento_nano_widget::toggle::THUMB_INSET_PX,
                    width: bento_nano_widget::toggle::THUMB_DIAMETER_PX,
                    height: bento_nano_widget::toggle::THUMB_DIAMETER_PX,
                };
                self.fill_rounded_rect(thumb_rect, t.thumb, t.thumb_radius)?;
            }
            WidgetNode::Radio(r) => {
                let selected = r.is_selected();
                let ring = if selected { r.ring_selected } else { r.ring };
                self.fill_rounded_rect(rect, ring, r.radius)?;
                let dot_progress = r.dot_progress();
                if dot_progress > 0.0 {
                    let dot_d = (rect.width * 0.5).max(0.0) * dot_progress;
                    let inset = (rect.width - dot_d) * 0.5;
                    let dot = bento_nano_style::Rect {
                        x: rect.x + inset,
                        y: rect.y + inset,
                        width: dot_d,
                        height: dot_d,
                    };
                    self.fill_rounded_rect(dot, r.dot, r.dot_radius_for_diameter(dot_d))?;
                }
            }
            WidgetNode::Slider(s) => {
                self.fill_rounded_rect(rect, s.track_color, s.track_radius)?;
                let value = (*s.value.get()).clamp(0.0, 1.0);
                let fill_rect = bento_nano_style::Rect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width * value,
                    height: rect.height,
                };
                self.fill_rounded_rect(fill_rect, s.fill_color, s.track_radius)?;
                let thumb_x = rect.x + rect.width * value
                    - bento_nano_widget::slider::THUMB_DIAMETER_PX * 0.5;
                let thumb_y =
                    rect.y + rect.height * 0.5 - bento_nano_widget::slider::THUMB_DIAMETER_PX * 0.5;
                let thumb = bento_nano_style::Rect {
                    x: thumb_x,
                    y: thumb_y,
                    width: bento_nano_widget::slider::THUMB_DIAMETER_PX,
                    height: bento_nano_widget::slider::THUMB_DIAMETER_PX,
                };
                self.fill_rounded_rect(thumb, s.thumb_color, s.thumb_radius)?;
            }
            WidgetNode::Input(i) => {
                let border = if i.focused { i.border_focus } else { i.border };
                self.fill_rounded_rect(rect, border, i.radius)?;
                self.fill_rounded_rect(rect, i.background, i.radius)?;
                let text_str = i.text.get().clone();
                if !text_str.is_empty() {
                    self.draw_text(text_str.as_str(), rect, i.text_color)?;
                } else if !i.placeholder.is_empty() {
                    self.draw_text(i.placeholder.as_str(), rect, i.placeholder_color)?;
                }
            }
            WidgetNode::Dropdown(d) => {
                let border = if d.popup.visible {
                    d.border_focus
                } else {
                    d.border
                };
                self.fill_rounded_rect(rect, border, d.radius)?;
                self.fill_rounded_rect(rect, d.background, d.radius)?;
                if let Some(label) = d.selected_label() {
                    self.draw_text(label, rect, d.text)?;
                }
            }
            WidgetNode::Tab(t) => {
                self.fill_rounded_rect(rect, t.header_color, BorderRadius::ZERO)?;
                let underline_x = rect.x + t.underline_anim.current();
                let underline_w = t.active_underline_width();
                let underline = bento_nano_style::Rect {
                    x: underline_x,
                    y: rect.y + rect.height - bento_nano_widget::tab::UNDERLINE_THICKNESS_PX,
                    width: underline_w,
                    height: bento_nano_widget::tab::UNDERLINE_THICKNESS_PX,
                };
                self.fill_rounded_rect(underline, t.underline_color, t.underline_radius)?;
            }
            WidgetNode::Collapsible(_) => {
                // Header + body are children dispatched by the outer loop;
                // the collapsible itself owns no fill — only the height
                // animation, which the layout engine reads directly.
            }
            WidgetNode::Modal(m) => {
                let alpha = m.fade_progress();
                if alpha > 0.0 {
                    let scrim = bento_nano_style::Color {
                        a: m.scrim.a * alpha,
                        ..m.scrim
                    };
                    self.fill_rounded_rect(rect, scrim, BorderRadius::ZERO)?;
                }
            }
            WidgetNode::Popup(_)
            | WidgetNode::Tooltip(_)
            | WidgetNode::ContextMenu(_)
            | WidgetNode::DragPreview(_) => {
                // Overlay primitives — they live in their own HWNDs (T-011
                // Window factory). The main-window render walk does not
                // paint them; per-window renderers handle their geometry.
            }
            WidgetNode::List(_)
            | WidgetNode::Grid(_)
            | WidgetNode::VirtualList(_)
            | WidgetNode::VirtualGrid(_)
            | WidgetNode::Row(_)
            | WidgetNode::Column(_)
            | WidgetNode::GridLayout(_) => {
                // Pure layout containers — children dispatched by the outer
                // iter loop. No own fill.
            }
            WidgetNode::SvgIcon(s) => {
                self.draw_svg_fit(s.source.as_str(), rect, s.tint, s.size)?;
            }
            WidgetNode::FileIcon(f) => {
                if !f.is_pending() {
                    // PHASE_2: pull bitmap from platform icon cache by
                    // `f.cache_hash`. Until the platform cache lands the
                    // background placeholder is correct.
                }
                if f.background.a > 0.0 {
                    self.fill_rounded_rect(rect, f.background, f.border_radius)?;
                }
            }
        }
        Ok(())
    }

    fn draw_stack_tray_overlay(&mut self, app: &AppState) -> Result<(), RenderError> {
        let Some(state) = app.stack_tray.borrow().clone() else {
            return Ok(());
        };
        let Some(anchor) = app.zones.get(state.anchor_zone_id) else {
            return Ok(());
        };
        let Some(member_ids) = app.zones.stack_member_ids(anchor.id) else {
            return Ok(());
        };
        // Wave D: consume Wave B Tauri-token SSoT for the tray panel chrome
        // instead of the legacy `bento-nano-theme` palette.
        let chrome = stack_tray::StackTrayChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let member_count = member_ids.len();
        let tray = stack_tray::stack_tray_rect(app.viewport, anchor, member_count);
        let tray_shadow = stack_tray::panel_shadow_rect(tray, chrome.panel_shadow);
        self.fill_rounded_rect(tray_shadow, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(tray, chrome.panel_background, chrome.panel_radius)?;

        self.draw_text(
            "StackTray",
            bento_nano_style::Rect {
                x: tray.x + stack_tray::TRAY_INSET_PX,
                y: tray.y + 10.0,
                width: 92.0,
                height: 18.0,
            },
            chrome.text_primary,
        )?;
        let count_label = format!("{member_count} members");
        self.draw_text(
            count_label.as_str(),
            bento_nano_style::Rect {
                x: tray.x + stack_tray::TRAY_INSET_PX + 96.0,
                y: tray.y + 11.0,
                width: 96.0,
                height: 16.0,
            },
            chrome.text_muted,
        )?;

        let dissolve = stack_tray::stack_tray_dissolve_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(dissolve, chrome.danger_background, chrome.button_radius)?;
        self.draw_text("Dissolve", inset_rect(dissolve, 5.0), chrome.text_primary)?;
        let close = stack_tray::stack_tray_close_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(close, chrome.button_background, chrome.button_radius)?;
        self.draw_text("Close", inset_rect(close, 5.0), chrome.text_primary)?;

        let selected_id = if member_ids.contains(&state.selected_member_id) {
            state.selected_member_id
        } else {
            member_ids[0]
        };
        let drag_state = app.stack_tray_drag.get();
        for (row_index, member_id) in member_ids
            .iter()
            .copied()
            .take(stack_tray::TRAY_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let Some(member) = app.zones.get(member_id) else {
                continue;
            };
            let row_rect =
                stack_tray::stack_tray_row_rect(app.viewport, anchor, member_count, row_index);
            self.fill_rounded_rect(
                row_rect,
                if drag_state.is_some_and(|drag| {
                    drag.anchor_zone_id == anchor.id && drag.member_id == member_id
                }) {
                    chrome.dragged_background
                } else if member_id == selected_id {
                    chrome.selected_background
                } else {
                    chrome.row_background
                },
                chrome.row_radius,
            )?;
            let icon_rect = bento_nano_style::Rect {
                x: row_rect.x + 8.0,
                y: row_rect.y + 8.0,
                width: 28.0,
                height: 22.0,
            };
            self.fill_rounded_rect(icon_rect, chrome.button_background, chrome.button_radius)?;
            self.draw_text(
                member.icon.as_ref(),
                bento_nano_style::Rect {
                    x: icon_rect.x + 6.0,
                    y: icon_rect.y + 3.0,
                    width: icon_rect.width - 12.0,
                    height: 14.0,
                },
                chrome.text_primary,
            )?;
            self.draw_text(
                member.title.as_ref(),
                bento_nano_style::Rect {
                    x: row_rect.x + 44.0,
                    y: row_rect.y + 7.0,
                    width: (row_rect.width - 128.0).max(0.0),
                    height: 16.0,
                },
                chrome.text_primary,
            )?;
            let item_count = member.items.len();
            let item_label = format!("{item_count} items");
            self.draw_text(
                item_label.as_str(),
                bento_nano_style::Rect {
                    x: row_rect.x + 44.0,
                    y: row_rect.y + 22.0,
                    width: (row_rect.width - 128.0).max(0.0),
                    height: 14.0,
                },
                chrome.text_muted,
            )?;
            let detach =
                stack_tray::stack_tray_detach_rect(app.viewport, anchor, member_count, row_index);
            self.fill_rounded_rect(detach, chrome.button_background, chrome.button_radius)?;
            self.draw_text("Detach", inset_rect(detach, 5.0), chrome.text_primary)?;
        }

        if member_count > stack_tray::TRAY_VISIBLE_ROW_LIMIT {
            let hidden = member_count - stack_tray::TRAY_VISIBLE_ROW_LIMIT;
            let label = format!("+{hidden} more members");
            self.draw_text(
                label.as_str(),
                bento_nano_style::Rect {
                    x: tray.x + stack_tray::TRAY_INSET_PX,
                    y: tray.bottom() - 18.0,
                    width: tray.width - stack_tray::TRAY_INSET_PX * 2.0,
                    height: 14.0,
                },
                chrome.text_muted,
            )?;
        } else if app.stack_tray_drag.get().is_some() {
            self.draw_text(
                "Drag over a row to reorder",
                bento_nano_style::Rect {
                    x: tray.x + stack_tray::TRAY_INSET_PX,
                    y: tray.bottom() - 18.0,
                    width: tray.width - stack_tray::TRAY_INSET_PX * 2.0,
                    height: 14.0,
                },
                chrome.text_accent,
            )?;
        } else if let Some(status) = state.status.as_ref() {
            self.draw_text(
                status.as_str(),
                bento_nano_style::Rect {
                    x: tray.x + stack_tray::TRAY_INSET_PX,
                    y: tray.bottom() - 18.0,
                    width: tray.width - stack_tray::TRAY_INSET_PX * 2.0,
                    height: 14.0,
                },
                chrome.text_accent,
            )?;
        }

        let Some(preview_zone) = app.zones.get(selected_id) else {
            return Ok(());
        };
        let preview = stack_tray::focused_preview_rect(app.viewport, tray);
        let preview_shadow = stack_tray::panel_shadow_rect(preview, chrome.panel_shadow);
        self.fill_rounded_rect(
            preview_shadow,
            chrome.panel_shadow.color,
            chrome.panel_radius,
        )?;
        self.fill_rounded_rect(preview, chrome.preview_background, chrome.panel_radius)?;
        self.draw_text(
            "FocusedZonePreview",
            bento_nano_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 12.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_accent,
        )?;
        self.draw_text(
            preview_zone.title.as_ref(),
            bento_nano_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 36.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_primary,
        )?;
        let geometry_label = format!(
            "{} · {}×{} · {} items",
            preview_zone.icon,
            preview_zone.w,
            preview_zone.h,
            preview_zone.items.len()
        );
        self.draw_text(
            geometry_label.as_str(),
            bento_nano_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 58.0,
                width: preview.width - 32.0,
                height: 16.0,
            },
            chrome.text_muted,
        )?;
        if preview_zone.items.is_empty() {
            self.draw_text(
                "No captured desktop items in this member.",
                bento_nano_style::Rect {
                    x: preview.x + 16.0,
                    y: preview.y + 92.0,
                    width: preview.width - 32.0,
                    height: 18.0,
                },
                chrome.text_muted,
            )?;
        } else {
            for (idx, item) in preview_zone.items.iter().take(4).enumerate() {
                let y = preview.y + 88.0 + idx as f32 * 24.0;
                let row = bento_nano_style::Rect {
                    x: preview.x + 16.0,
                    y,
                    width: preview.width - 32.0,
                    height: 20.0,
                };
                self.fill_rounded_rect(row, chrome.row_background, chrome.preview_item_radius)?;
                self.draw_text(
                    item.name.as_ref(),
                    bento_nano_style::Rect {
                        x: row.x + 8.0,
                        y: row.y + 3.0,
                        width: row.width - 16.0,
                        height: 13.0,
                    },
                    chrome.text_primary,
                )?;
            }
        }
        Ok(())
    }

    // α5 (S2, 2026-05-24): no longer called from the Main HWND paint loop
    // (the unconditional call at :470 leaked a 4 DIP blue strip across the
    // top of the desktop overlay). Kept as `dead_code`-tolerant in case a
    // future Settings header or accent-callout reuses it; `cargo test` still
    // pins the math at :1235/1283/1303/1391 via the consumer accessors.
    #[allow(dead_code)]
    fn draw_theme_base_accent(&mut self, app: &AppState) -> Result<(), RenderError> {
        let accent = app
            .theme_base_accent
            .borrow()
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| with_alpha(app.active_theme_palette().accent, 0.92));
        let rect = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: app.viewport.width,
            height: 4.0,
        };
        self.fill_rounded_rect(rect, accent, BorderRadius::ZERO)
    }

    /// Draw all zones from `app.zones`. Each zone is a translucent rounded
    /// rectangle with its title at top-left. Zones live in their own
    /// collection (Ruling 2) and rendering walks the list directly — no
    /// widget-tree mount.
    fn draw_zones(&mut self, app: &AppState) -> Result<(), RenderError> {
        // V-8 — wall-clock used to sample the pill animator. We read
        // `GetTickCount` once per frame so all pills share the same phase
        // (the breathing dot looks broken if each pill samples a different
        // `now`). Allocation-free per spec §10.
        // SAFETY: `GetTickCount` is total + thread-safe.
        let anim_now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let palette = app.active_theme_palette();
        // M6a — live Tauri-parity palette for this frame. Bound ONCE here and
        // threaded into the pill / morph paint helpers so the whole zone
        // surface re-skins with the active theme (§10: Copy, no re-borrow).
        let pal = app.active_theme_tauri();
        // M6b — active theme's Tauri-parity shadow stacks (Copy, bound once §10).
        // The expanded-panel drop band + the collapsed-pill zen halo both read
        // their per-theme stack from here so e.g. `terminal`'s green glow and
        // the Angular `none` themes' empty stacks paint correctly.
        let shadow_tauri = app.active_theme_shadow_tauri();
        // M6c — active theme's effect channel (Copy, bound once §10). Only
        // `cyberpunk` (Neon) consumes it here, layering an ADDITIVE bloom on
        // top of the M6b box-shadow; every other theme no-ops at the variant
        // match.
        let effect = app.active_theme_effect_tauri();
        let zone_chrome =
            zone_surface_geometry::ZoneSurfaceChrome::from_radius(app.active_theme_radius());
        let item_chrome = item_card::ItemCardChrome::from_tokens(
            palette,
            app.active_theme_radius(),
            pal.surface_subtle,
            pal.text_secondary,
        );
        // V-9 round 4 (2026-05-21) — Tauri 1.2.4 reference (frame_010 / 015
        // of resource/屏幕录制 2026-05-20 161936.mp4) paints the expanded
        // zone surface with `--surface-dialog` (rgba 12,12,18,0.92) AND
        // honours its natural 0xEB alpha so the desktop wallpaper bleeds
        // through as a heavily dimmed acrylic-style veil. Round 3 forced
        // alpha to 1.0 → completely opaque slab, fails parity (user audit
        // 2026-05-21 said "这和原版动效完全不一样"). Drop the alpha clamp
        // and use the token directly. The 0.38/0.34 baseline (pre-round-2)
        // was the opposite failure: too transparent, desktop icons visible
        // at full saturation. The 0.92 token alpha is the Tauri-pinned mid.
        let zone_fill_idle = pal.surface_dialog;
        let zone_fill_active = with_alpha(palette.accent, 0.92);
        let zone_title = with_alpha(palette.text, 0.88);
        let zone_icon_chip = with_alpha(palette.surface_alt, 0.58);
        let zone_icon_text = with_alpha(palette.text, 0.92);
        let zone_live_folder_text = with_alpha(palette.text_muted, 0.94);
        let stack_shadow = with_alpha(palette.accent, 0.14);
        let stack_wrapper_halo = with_alpha(palette.accent, 0.10);
        let stack_badge_fill = with_alpha(palette.accent, 0.82);
        let stack_peek_fill = with_alpha(palette.surface_alt, 0.78);
        let zone_drop_target_glow = with_alpha(palette.accent_hover, 0.30);
        let drop_preview_fill = with_alpha(palette.accent, 0.20);
        let drop_preview_core = with_alpha(palette.accent_hover, 0.34);
        let radius = zone_chrome.zone_radius;
        let active_id = app
            .zone_drag
            .get()
            .map(|t| t.0)
            .or_else(|| app.zone_resize.get().map(|t| t.0));
        let item_drag = active_item_drag_visual(app);
        let drag_target_id =
            item_drag.and_then(|drag| hit_test_render_zone(app, drag.last_x, drag.last_y));
        let dragged_item_wide = item_drag
            .and_then(|drag| {
                app.zones
                    .item(drag.zone_id, drag.item_id)
                    .map(|item| item.is_wide)
            })
            .unwrap_or(false);
        let theme_base_accent = app.theme_base_accent.borrow().clone();
        for zone in app.zones.iter() {
            if !zone.is_visible() || zone.is_stacked_child() {
                continue;
            }
            // Wave C (05-20 visual parity) — collapsed pill render path.
            // Stack anchors keep the legacy halo + peek chrome below; every
            // other zone whose `body_visible_for_mode` is false renders as a
            // Tauri-style capsule pill at `(zone.x, zone.y)` consuming the
            // Wave B token SSoT in `zone_pill_geometry`.
            let pill_body_visible =
                app.zone_body_visible_for_mode(zone) || Some(zone.id) == active_id;
            // Wave G2 — morphing capsule. When the hover transition is
            // in-flight for this zone, paint an intermediate rounded-rect
            // instead of snapping between collapsed pill and expanded body.
            // Stack anchors keep their bespoke chrome and skip the morph.
            let pill_anim_active = app.zone_pill_anim_zone.get() == Some(zone.id)
                && !zone.is_stack_anchor()
                && {
                    let p = app.zone_pill_anim_progress.get();
                    p > 0.0 && p < 1.0
                };
            if pill_anim_active {
                let count = zone.items.len();
                let pill_layout =
                    zone_pill_geometry::pill_layout_for_zone(zone, count);
                let expanded_rect = bento_nano_style::Rect {
                    x: zone.x as f32,
                    y: zone.y as f32,
                    width: zone.w as f32,
                    height: zone.h as f32,
                };
                let raw = app.zone_pill_anim_progress.get();
                // M3 (2026-05-29) — Tauri `.spring-expand` easeOutBack curve
                // (cubic-bezier(0.34,1.56,0.64,1)). `eased` overshoots ~10%
                // past 1.0 mid-flight then settles exactly to 1.0; the
                // overshoot flows through `morph_pill_to_rect`/`_radius`
                // (which no longer upper-clamp) so the rect+radius bulge then
                // snap back, 1:1 with the Tauri capsule<->panel transition.
                let eased = zone_pill_geometry::ease_out_back_progress(raw);
                // `expanding` true → morph from pill (0) to expanded (1);
                // false → morph from expanded (0) to pill (1). We flip when
                // collapsing so the same morph helper still produces the
                // right intermediate rect.
                let morph = if app.zone_pill_anim_expanding.get() {
                    eased
                } else {
                    1.0 - eased
                };
                // B2 (2026-05-29) — Tauri drives `background`/`border-color`
                // on a SEPARATE 0.3s CSS-`ease` timeline (animations.css:44-45),
                // NOT the 0.5s easeOutBack size curve. The morph `raw` fraction
                // spans the full 500ms; the color transition completes in the
                // FIRST 300ms, so we rescale raw by 500/300 (clamped) then run
                // the CSS-`ease` curve. Collapse reverses it in lockstep with
                // the size morph so the channels stay phase-aligned.
                let color_raw = (raw * 500.0 / 300.0).min(1.0);
                let color_eased = zone_pill_geometry::ease_standard_progress(color_raw);
                let color_t = if app.zone_pill_anim_expanding.get() {
                    color_eased
                } else {
                    1.0 - color_eased
                };
                self.draw_zone_pill_morph(
                    zone,
                    &pill_layout,
                    expanded_rect,
                    morph,
                    color_t,
                    theme_base_accent.as_deref(),
                    pal,
                    effect,
                )?;
                continue;
            }
            if !pill_body_visible && !zone.is_stack_anchor() {
                let count = zone.items.len();
                let layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
                // V-8 — sample hover / press channels at paint time. The
                // animator borrow is released before any further mutation
                // (the pill paint helpers are read-only on app state).
                let (hover_t, press_t) = {
                    let anim = app.pill_animator.borrow();
                    (
                        anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms),
                        anim.sample(zone.id, animator::AnimChannel::PillPress, anim_now_ms),
                    )
                };
                self.draw_zone_pill(
                    zone,
                    &layout,
                    theme_base_accent.as_deref(),
                    hover_t,
                    press_t,
                    anim_now_ms,
                    pal,
                    effect,
                )?;
                continue;
            }
            let rect = bento_nano_style::Rect {
                x: zone.x as f32,
                y: zone.y as f32,
                width: zone.w as f32,
                height: zone.h as f32,
            };
            // Wave I2 — expanded body chrome (panel shadow / header band /
            // divider / count badge). M2 (05-29): the footer thumbnail strip
            // (E-01) was deleted — Tauri's BentoPanel has no footer node.
            // `stack_member_ids_for_anchor` is still bound here because the
            // stack-anchor halo / "Stack ×N" badge / peek chrome below all
            // read it. Stack anchors keep their bespoke halo + shadow chrome
            // below; the shadow band is suppressed for anchors so we don't
            // double-stamp shadows.
            let stack_member_ids_for_anchor = app.zones.stack_member_ids(zone.id);
            let expanded_layout = expanded_zone_grid::expanded_zone_layout(zone);
            if !zone.is_stack_anchor() {
                // M6b — per-theme `expanded` stack under the panel band so the
                // expanded surface lifts off the desktop backdrop. `draw_shadow_stack`
                // grows the panel base rect per layer (the Angular `none` themes
                // paint nothing here; tinted Rounded themes carry their L2 colour).
                self.draw_shadow_stack(expanded_layout.panel, shadow_tauri.expanded, radius)?;
                // M6c — the `cyberpunk` neon `filter: drop-shadow` bloom on the
                // expanded panel (`.bento-zone-expanded`), ADDITIVE on top of
                // the M6b box-shadow above and UNDER the surface fill below.
                if let bento_nano_style::tokens::EffectTauri::Neon(n) = effect {
                    self.draw_neon_glow(expanded_layout.panel, n.expanded, radius)?;
                }
            }
            if zone.is_stack_anchor() {
                let member_count = stack_member_ids_for_anchor
                    .as_ref()
                    .map(|ids| ids.len())
                    .unwrap_or(1);
                let halo_rect = stack_tray::stack_wrapper_halo_rect(zone, member_count);
                self.fill_rounded_rect(
                    halo_rect,
                    stack_wrapper_halo,
                    zone_chrome.stack_halo_radius,
                )?;
                for offset in [8.0_f32, 4.0_f32] {
                    let shadow_rect = bento_nano_style::Rect {
                        x: rect.x + offset,
                        y: rect.y + offset,
                        width: rect.width,
                        height: rect.height,
                    };
                    self.fill_rounded_rect(shadow_rect, stack_shadow, radius)?;
                }
            }
            if Some(zone.id) == drag_target_id {
                let glow_rect = bento_nano_style::Rect {
                    x: rect.x - 3.0,
                    y: rect.y - 3.0,
                    width: rect.width + 6.0,
                    height: rect.height + 6.0,
                };
                self.fill_rounded_rect(
                    glow_rect,
                    zone_drop_target_glow,
                    zone_chrome.drop_target_radius,
                )?;
            }
            let fill = if Some(zone.id) == active_id {
                zone_fill_active
            } else {
                zone_fill_idle
            };
            self.fill_rounded_rect(rect, fill, radius)?;
            // M2③ (05-31, ruling = A / 1:1) — re-add the 2px top accent edge
            // that V-9 (2026-05-21) removed. Authoritative source is Tauri
            // `.bento-zone--expanded { border-top: 2px solid var(--zone-accent,
            // transparent) }` (BentoZone.css:113-114). `--zone-accent` is
            // injected ONLY from `zone.accent_color` (BentoZone.tsx:1409-1410);
            // when the zone has no accent the border resolves to `transparent`,
            // i.e. nothing is painted. So we match 1:1: paint a full-alpha 2px
            // bar in the zone's own accent colour, and skip it entirely (no
            // theme-base fallback) when the zone defines no accent.
            //
            // The bar is inset horizontally by the panel corner radius so it
            // runs across the flat top span between the two rounded corners,
            // mirroring how CSS `border-top` follows `border-radius` without
            // bleeding past the arcs. `fill_rounded_rect` short-circuits on
            // `color.a <= 0.0`, so the `None`/transparent case is a true no-op.
            if let Some(accent) = zone.accent_color.as_deref().and_then(parse_hex_color) {
                let accent_inset = radius.top_left.min(rect.width * 0.5);
                let accent_edge = bento_nano_style::Rect {
                    x: rect.x + accent_inset,
                    y: rect.y,
                    width: (rect.width - accent_inset * 2.0).max(0.0),
                    height: PANEL_ACCENT_EDGE_THICKNESS_PX,
                };
                self.fill_rounded_rect(
                    accent_edge,
                    with_alpha(accent, 1.0),
                    bento_nano_style::BorderRadius::ZERO,
                )?;
            }
            // M2③ (05-31, 1:1) — Tauri `.panel-header` is `height: 48px` with
            // `align-items: center` (PanelHeader.css:6,5). The icon chip is
            // vertically centred in the 48-DIP band: top = (48 - 18)/2 = 15.
            let icon_rect = bento_nano_style::Rect {
                x: rect.x + 8.0,
                y: rect.y + 15.0,
                width: 68.0,
                height: 18.0,
            };
            self.fill_rounded_rect(icon_rect, zone_icon_chip, zone_chrome.icon_chip_radius)?;
            let icon_text_rect = bento_nano_style::Rect {
                x: icon_rect.x + 6.0,
                y: icon_rect.y + 2.0,
                width: icon_rect.width - 12.0,
                height: icon_rect.height - 4.0,
            };
            // RC-4 Gap 1 — render the zone icon as a line-art glyph rather
            // than the raw wire-format name.
            self.draw_icon_glyph(zone.icon.as_ref(), icon_text_rect, zone_icon_text)?;
            // M2③ — title vertically centred in the 48-DIP header band
            // (Tauri `align-items: center`): top = (48 - 18)/2 = 15.
            let title_rect = bento_nano_style::Rect {
                x: rect.x + 82.0,
                y: rect.y + 15.0,
                width: (rect.width - 90.0).max(0.0),
                height: 18.0,
            };
            self.draw_text(&zone.title, title_rect, zone_title)?;
            let body_visible = app.zone_body_visible_for_mode(zone) || Some(zone.id) == active_id;
            // M2 E-02 (2026-05-29) — Tauri's `PanelHeader` carries an item
            // COUNT BADGE (`.panel-header__badge`), NOT a status dot. The
            // V-14 green status dot was DELETED here (Tauri's expanded header
            // has no dot). The badge mirrors the ZenCapsule badge style:
            // radius 10 (`RADIUS.badge`), bg `var(--zone-accent, --badge-bg)`,
            // 11px count text in `--text-primary`. Right-aligned in the
            // header band. Skipped for stack anchors — the "Stack ×N" badge
            // below claims the same top-right slot (avoids overlap).
            if !zone.is_stack_anchor() {
                let badge_rect = expanded_layout.header_badge;
                if badge_rect.width > 0.0 && badge_rect.height > 0.0 {
                    // Prefer the zone's accent tint (Tauri `--zone-accent`),
                    // falling back to the neutral `--badge-bg`.
                    let badge_fill = zone
                        .accent_color
                        .as_deref()
                        .or(theme_base_accent.as_deref())
                        .and_then(parse_hex_color)
                        .unwrap_or(pal.badge_bg);
                    self.fill_rounded_rect(
                        badge_rect,
                        badge_fill,
                        bento_nano_style::BorderRadius::all(
                            app.active_theme_radius_tauri().badge,
                        ),
                    )?;
                    let count_str = format_small_count(zone.items.len());
                    let count_rect = bento_nano_style::Rect {
                        x: badge_rect.x + 4.0,
                        y: badge_rect.y + 2.0,
                        width: (badge_rect.width - 8.0).max(0.0),
                        height: (badge_rect.height - 4.0).max(0.0),
                    };
                    self.draw_text(
                        count_str.as_str(),
                        count_rect,
                        pal.text_primary,
                    )?;
                }
            }
            // V-11 (2026-05-21, round 2): the expanded-zone right-bottom
            // display-mode chip ("Hover"/"Always"/"Click") was deleted.
            // Tauri 1.2.4 baseline never paints a display-mode label on the
            // zone surface — the mode is toggled exclusively through the
            // Settings panel's ZoneDisplay row (SettingsHit::CycleZoneDisplayMode,
            // dispatched at bento-nano-shell/src/main.rs:11465 and :12907).
            // The `ZoneSurfaceChrome::display_chip_radius` token + the
            // `effective_zone_display_mode` accessor on AppState are kept for
            // log/test parity; M4 owns the K1 dead_code sweep for the now-
            // unused chrome field.
            if zone.is_stack_anchor() {
                let member_ids = app.zones.stack_member_ids(zone.id);
                let member_count = member_ids.as_ref().map(|ids| ids.len()).unwrap_or(1);
                // M2③ cascade — the "Stack ×N" badge sits in the sub-row just
                // below the header band; it tracks the header height so it
                // stays clear of the taller 48-DIP header (was y+34 under the
                // legacy 30-DIP band; now header bottom + 4 = 48 + 4 = 52).
                let stack_subrow_y = item_grid::ITEM_GRID_TOP_OFFSET_PX + 4.0;
                let badge_rect = bento_nano_style::Rect {
                    x: rect.right() - 76.0,
                    y: rect.y + stack_subrow_y,
                    width: 68.0,
                    height: 18.0,
                };
                self.fill_rounded_rect(
                    badge_rect,
                    stack_badge_fill,
                    zone_chrome.stack_badge_radius,
                )?;
                let badge_label = format!("Stack ×{member_count}");
                self.draw_text(
                    badge_label.as_str(),
                    bento_nano_style::Rect {
                        x: badge_rect.x + 7.0,
                        y: badge_rect.y + 2.0,
                        width: badge_rect.width - 14.0,
                        height: 12.0,
                    },
                    zone_icon_text,
                )?;
                if let Some(member_ids) = member_ids {
                    if let Some(member) = member_ids
                        .iter()
                        .copied()
                        .find(|member_id| *member_id != zone.id)
                        .and_then(|member_id| app.zones.get(member_id))
                    {
                        // M2③ cascade — peek row trails the Stack badge by the
                        // same 20-DIP step it used under the legacy header.
                        let peek_rect = bento_nano_style::Rect {
                            x: rect.x + 8.0,
                            y: rect.y + stack_subrow_y + 20.0,
                            width: (rect.width - 16.0).max(0.0),
                            height: 18.0,
                        };
                        self.fill_rounded_rect(
                            peek_rect,
                            stack_peek_fill,
                            zone_chrome.stack_peek_radius,
                        )?;
                        let peek_label = format!("Peek: {}", member.title);
                        self.draw_text(
                            peek_label.as_str(),
                            bento_nano_style::Rect {
                                x: peek_rect.x + 7.0,
                                y: peek_rect.y + 2.0,
                                width: peek_rect.width - 14.0,
                                height: 12.0,
                            },
                            zone_live_folder_text,
                        )?;
                    }
                }
            }
            if !body_visible {
                continue;
            }
            // Wave I2 / M2 E-04 — divider hairline between the header band
            // and the item grid. Tauri's `.panel-header` border-bottom is
            // `rgba(255,255,255,0.05)` — pure WHITE at alpha 0.05, NOT the
            // tinted `palette.text` at 0.10 (which read 2× too strong and
            // slightly warm). Corrected to match exactly.
            self.fill_rounded_rect(
                expanded_layout.divider,
                with_alpha(bento_nano_style::Color::WHITE, 0.05),
                bento_nano_style::BorderRadius::ZERO,
            )?;
            // V-9 round 2 (2026-05-21) — expanded-body status dot removed.
            // User flagged it as a stray blue ring above each pill ("4" / "10").
            // Tauri 1.2.4 expanded panel has no top-right indicator; the
            // collapsed pill keeps its Wave H2 dot since that one matches
            // baseline.
            if let Some(path) = zone.live_folder_path.as_deref() {
                let live_text = live_folder_badge_text(path);
                // M2③ cascade — live-folder badge sits just below the 48-DIP
                // header band (was y+34 under the legacy 30-DIP header).
                let live_rect = bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    y: rect.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + 4.0,
                    width: (rect.width - 16.0).max(0.0),
                    height: 16.0,
                };
                self.fill_rounded_rect(live_rect, zone_icon_chip, zone_chrome.live_badge_radius)?;
                self.draw_text(
                    live_text.as_str(),
                    bento_nano_style::Rect {
                        x: live_rect.x + 6.0,
                        y: live_rect.y + 2.0,
                        width: (live_rect.width - 12.0).max(0.0),
                        height: 12.0,
                    },
                    zone_live_folder_text,
                )?;
            }
            if Some(zone.id) == drag_target_id {
                if let Some(preview) =
                    drop_preview_rect_for_zone(zone, item_drag, dragged_item_wide)
                {
                    self.fill_rounded_rect(preview, drop_preview_fill, item_chrome.card_radius)?;
                    let core = inset_rect(preview, 4.0);
                    self.fill_rounded_rect(
                        core,
                        drop_preview_core,
                        zone_chrome.drop_preview_core_radius,
                    )?;
                }
            }
            for item in &zone.items {
                let card_rect = item_card_rect_for_grid(zone, item.x, item.y, item.is_wide);
                if card_rect.width <= 0.0 || card_rect.height <= 0.0 {
                    continue;
                }
                let is_dragged_source = item_drag
                    .map(|drag| drag.zone_id == zone.id && drag.item_id == item.id)
                    .unwrap_or(false);
                let item_fill = if is_dragged_source {
                    item_chrome.drag_source_background
                } else if item.file_missing {
                    item_chrome.missing_background
                } else {
                    item_chrome.normal_background
                };
                // M3-A2 — sample the live per-item hover/press ramp and compose
                // the Tauri scale(1.02)/scale(0.97). The dragged source card
                // never scales (it's the muted placeholder under the ghost),
                // so it stays at identity. `item_hover` is `Copy` in a `Cell`,
                // so this is a single read + a few muls per card (§10 hot path).
                let item_scale = if is_dragged_source {
                    1.0
                } else {
                    let (hover_t, press_t) =
                        app.item_hover.get().sample((zone.id, item.id), anim_now_ms);
                    item_card::card_scale_for(hover_t, press_t)
                };
                self.draw_item_card(
                    item,
                    card_rect,
                    item_fill,
                    item_chrome.card_radius,
                    item_chrome.text,
                    item_chrome.icon_text,
                    item_scale,
                )?;
            }
            // M2 E-01 (2026-05-29) — the 16×16 sub-zone footer thumbnail
            // strip was DELETED. Tauri's `BentoPanel` renders header + grid
            // only with no footer node; the strip was an additive nano
            // divergence visible only on stack anchors. Removed for 1:1.
        }
        if let Some(anchor_id) = app
            .hovered_zone
            .get()
            .and_then(|zone_id| app.zones.stack_anchor_for(zone_id))
        {
            if let Some(anchor) = app.zones.get(anchor_id) {
                if let Some(member_ids) = app.zones.stack_member_ids(anchor.id) {
                    let reveal_progress = if app.stack_bloom_anchor.get() == Some(anchor.id) {
                        app.stack_bloom_progress.get()
                    } else {
                        1.0
                    };
                    let frames = stack_tray::stack_bloom_frames_at(
                        app.viewport,
                        anchor,
                        member_ids.len(),
                        reveal_progress,
                    );
                    for (index, (member_id, frame)) in member_ids
                        .iter()
                        .copied()
                        .zip(frames.iter().copied())
                        .enumerate()
                    {
                        let Some(member) = app.zones.get(member_id) else {
                            continue;
                        };
                        let petal_rect = frame.rect;
                        if frame.connector.width > 0.5 && frame.connector.height > 0.5 {
                            self.fill_rounded_rect(
                                frame.connector,
                                with_alpha(palette.accent, 0.16 * frame.alpha),
                                zone_chrome.bloom_connector_radius,
                            )?;
                        }
                        let shadow_rect = bento_nano_style::Rect {
                            x: petal_rect.x + 3.0,
                            y: petal_rect.y + 4.0,
                            width: petal_rect.width,
                            height: petal_rect.height,
                        };
                        self.fill_rounded_rect(
                            shadow_rect,
                            with_alpha(palette.scrim, 0.20 * frame.alpha),
                            zone_chrome.bloom_petal_radius,
                        )?;
                        self.fill_rounded_rect(
                            petal_rect,
                            with_alpha(palette.surface_alt, 0.62 + 0.26 * frame.alpha),
                            zone_chrome.bloom_petal_radius,
                        )?;
                        let border_rect = bento_nano_style::Rect {
                            x: petal_rect.x + 2.0,
                            y: petal_rect.y + 2.0,
                            width: (petal_rect.width - 4.0).max(0.0),
                            height: (petal_rect.height - 4.0).max(0.0),
                        };
                        self.fill_rounded_rect(
                            border_rect,
                            with_alpha(palette.accent, 0.16 + 0.12 * frame.alpha),
                            zone_chrome.bloom_border_radius,
                        )?;
                        let core_rect = bento_nano_style::Rect {
                            x: petal_rect.x + 4.0,
                            y: petal_rect.y + 4.0,
                            width: (petal_rect.width - 8.0).max(0.0),
                            height: (petal_rect.height - 8.0).max(0.0),
                        };
                        self.fill_rounded_rect(
                            core_rect,
                            with_alpha(palette.surface_alt, 0.74 + 0.14 * frame.alpha),
                            zone_chrome.bloom_core_radius,
                        )?;
                        let index_rect = bento_nano_style::Rect {
                            x: petal_rect.x + 7.0,
                            y: petal_rect.y + 6.0,
                            width: 20.0,
                            height: 16.0,
                        };
                        self.fill_rounded_rect(
                            index_rect,
                            with_alpha(palette.accent, 0.68 + 0.18 * frame.alpha),
                            zone_chrome.bloom_index_radius,
                        )?;
                        let index_label = (index + 1).to_string();
                        self.draw_text(
                            index_label.as_str(),
                            bento_nano_style::Rect {
                                x: index_rect.x + 6.0,
                                y: index_rect.y + 2.0,
                                width: index_rect.width - 12.0,
                                height: 10.0,
                            },
                            zone_icon_text,
                        )?;
                        let label = format!("{} {}", member.icon, member.title);
                        self.draw_text(
                            label.as_str(),
                            bento_nano_style::Rect {
                                x: petal_rect.x + 34.0,
                                y: petal_rect.y + 6.0,
                                width: (petal_rect.width - 42.0).max(0.0),
                                height: 14.0,
                            },
                            zone_title,
                        )?;
                    }
                    if member_ids.len() > stack_tray::BLOOM_VISIBLE_PETAL_LIMIT {
                        let hidden = member_ids.len() - stack_tray::BLOOM_VISIBLE_PETAL_LIMIT;
                        if let Some(last) = frames.last().copied() {
                            let overflow_rect = bento_nano_style::Rect {
                                x: last.rect.x,
                                y: last.rect.bottom() + 4.0,
                                width: last.rect.width,
                                height: 18.0,
                            };
                            let overflow_label = format!("+{hidden} more stack members");
                            self.draw_text(
                                overflow_label.as_str(),
                                overflow_rect,
                                zone_live_folder_text,
                            )?;
                        }
                    }
                }
            }
        }
        if let Some(drag) = item_drag {
            if let Some((zone, item)) = source_drag_item(app, drag) {
                let source_rect = item_card_rect_for_grid(zone, item.x, item.y, item.is_wide);
                let ghost_rect = drag_ghost_rect(app, drag, source_rect);
                let shadow_rect = bento_nano_style::Rect {
                    x: ghost_rect.x + 4.0,
                    y: ghost_rect.y + 6.0,
                    width: ghost_rect.width,
                    height: ghost_rect.height,
                };
                self.fill_rounded_rect(
                    shadow_rect,
                    item_chrome.ghost_shadow,
                    item_chrome.card_radius,
                )?;
                self.draw_item_card(
                    item,
                    ghost_rect,
                    if item.file_missing {
                        item_chrome.missing_background
                    } else {
                        item_chrome.ghost_background
                    },
                    item_chrome.card_radius,
                    item_chrome.text,
                    item_chrome.icon_text,
                    // M3-A2 — the floating drag ghost is not a hover target;
                    // it keeps identity scale (the ghost has its own lift/shadow
                    // treatment) so hover/press scaling stays on the live grid.
                    1.0,
                )?;
            }
        }
        // V-11 (2026-05-21): bottom-left `item_operation_status` chip removed.
        // Tauri 1.2.4 baseline never painted a status pill on item open/copy/etc;
        // the `AppState::item_operation_status` cell + `ZoneSurfaceChrome::
        // item_status_radius` token are kept for log/test parity (and a possible
        // future toast surface) but are no longer rendered. M4 owns the dead_code
        // sweep for the now-unused field.
        Ok(())
    }

    /// Wave C (05-20 visual parity) — collapsed zone pill render path.
    /// Tauri 1.2.4 shows each zone as a rounded capsule (icon + name + count
    /// badge with `SHADOW.zen` outer / inner two-layer drop) by default; this
    /// method consumes the Wave B token SSoT (`PALETTE_DARK`, `RADIUS`,
    /// `SHADOW`, `ACRYLIC_FALLBACK`) and `zone_pill_geometry::ZonePillLayout`
    /// to paint that surface in our D2D pump. Per parent PRD D5, acrylic is
    /// the solid `ACRYLIC_FALLBACK` tint only.
    ///
    /// M6a — the live `pal: PaletteTauri` is the 8th arg, threaded in from
    /// `draw_zones` (bound once per frame, §10) so the pill re-skins with the
    /// active theme. The paint inputs (zone / layout / anim channels / palette)
    /// are all genuinely distinct, so the arity is allowed rather than bundled.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn draw_zone_pill(
        &mut self,
        zone: &Zone,
        layout: &ZonePillLayout,
        theme_base_accent: Option<&str>,
        hover_t: f32,
        press_t: f32,
        anim_now_ms: u32,
        pal: bento_nano_style::tokens::PaletteTauri,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — the live theme palette is passed in by `draw_zones` (bound
        // once per frame). Read `pal.X` instead of the static `PALETTE_DARK`
        // so the collapsed pill re-skins with the active theme.
        use bento_nano_style::tokens::ACRYLIC_FALLBACK;
        // V-8 — compose hover + press into the final scale multiplier and
        // expand the pill rect about its center. Persisted geometry tokens
        // are NEVER mutated (hard constraint) — `scale_rect_centered`
        // returns a fresh `Rect` for paint only.
        let scale = animator::pill_scale_for(hover_t, press_t);
        let scaled_rect = animator::scale_rect_centered(layout.rect, scale);
        let scaled_radius = layout.radius;
        // V-9 (2026-05-21) — Tauri 1.2.4 reference pills have NO drop-shadow
        // halo around the capsule. The previous SHADOW.zen / zen_inner fills
        // produced a soft translucent "backdrop" that user flagged as
        // "蒙版层 / backdrop". Removed entirely; the pill now reads as a
        // clean capsule against the desktop. Shadow layout fields are kept
        // in `ZonePillLayout` for the expanded morph path which still needs
        // the geometry, but the collapsed paint no longer fills them.
        // M6c — the `cyberpunk` neon `filter: drop-shadow` bloom on the
        // collapsed pill (`.bento-zone`), painted UNDER the glass+surface fill.
        // No M6b shadow is drawn on the collapsed pill (V-9 removed it), so this
        // is the only glow on the capsule — additive-by-design, no conflation.
        if let bento_nano_style::tokens::EffectTauri::Neon(n) = effect {
            self.draw_neon_glow(scaled_rect, n.collapsed, scaled_radius)?;
        }
        // Acrylic glass tint per parent PRD D5 — solid fallback, never Mica.
        self.fill_rounded_rect(scaled_rect, ACRYLIC_FALLBACK, scaled_radius)?;
        // Surface fill on top of the glass — `pal.surface_zen` (live theme).
        // V-8 — hover brightens the surface tone subtly (+8% on hover).
        let surface_brighten = 1.0 + hover_t * 0.08;
        let surface_color = Color {
            r: (pal.surface_zen.r * surface_brighten).min(1.0),
            g: (pal.surface_zen.g * surface_brighten).min(1.0),
            b: (pal.surface_zen.b * surface_brighten).min(1.0),
            a: pal.surface_zen.a,
        };
        self.fill_rounded_rect(scaled_rect, surface_color, scaled_radius)?;
        // M2 S2a (2026-05-29) — Tauri's `.zen-capsule` carries a 1px solid
        // `var(--border-zen)` = `rgba(255,255,255,0.1)` outline. nano drew no
        // stroke at all; added here so the capsule reads as glass with a
        // hairline edge. Pure-paint via the existing `stroke_rounded_rect`.
        self.stroke_rounded_rect(
            scaled_rect,
            pal.border_zen,
            scaled_radius,
            1.0,
        )?;
        // M2 S2b (2026-05-29) — the under-icon accent stripe was REMOVED.
        // Tauri's collapsed ZenCapsule has no such stripe (the 2px accent
        // border-top belongs to the EXPANDED body only). The zone accent is
        // still consulted below to tint the count badge (Tauri
        // `var(--zone-accent, --badge-bg)`).
        let accent_hex = zone.accent_color.as_deref().or(theme_base_accent);
        // Icon glyph — Tauri renders zone.icon as a 24×24 line-art SVG.
        // RC-4 Gap 1 — switched from `draw_text(zone.icon)` (which drew the
        // raw name like "settings" → DWrite wrapped it to "set tin gs") to
        // `draw_icon_glyph`, which looks up the built-in `IconKind` and
        // renders the cached source SVG. Unknown names fall back to text.
        self.draw_icon_glyph(zone.icon.as_ref(), layout.icon, pal.text_primary)?;
        // Label — zone.title in TEXT_PRIMARY. M2 R3 (2026-05-29) — Tauri's
        // `.zen-capsule__title` is single-line `white-space:nowrap`; the
        // proportional `draw_text` wrapped long names onto a second row.
        // Switched to `draw_text_no_wrap`, which disables DWrite word-wrap
        // and `…`-trims when the glyph run overflows — matching Tauri's
        // shrink-then-clip behaviour without the wrap regression.
        self.draw_text_no_wrap(zone.title.as_ref(), layout.label, pal.text_primary)?;
        // Count badge — M2 B3 (2026-05-29): bg follows Tauri
        // `var(--zone-accent, --badge-bg)` (zone accent tint when set, else
        // the neutral `--badge-bg`), and the count text is `--text-primary`
        // (#f0f0f5) NOT the dimmer `--text-secondary` it used before.
        let count = zone.items.len();
        let badge_fill = accent_hex
            .and_then(parse_hex_color)
            .unwrap_or(pal.badge_bg);
        self.fill_rounded_rect(
            layout.badge,
            badge_fill,
            layout.badge_radius,
        )?;
        let count_str = format_small_count(count);
        let badge_text_rect = bento_nano_style::Rect {
            x: layout.badge.x + 4.0,
            y: layout.badge.y + 2.0,
            width: (layout.badge.width - 8.0).max(0.0),
            height: (layout.badge.height - 4.0).max(0.0),
        };
        self.draw_text(
            count_str.as_str(),
            badge_text_rect,
            pal.text_primary,
        )?;
        // V-9 round 3 (2026-05-21) — Wave H2 status dot removed. User
        // flagged the blue dot at the top of every collapsed pill as a
        // regression vs Tauri 1.2.4 baseline (Tauri pills have no such
        // indicator — `count > 0` is communicated entirely through the
        // numeric badge on the right).
        //
        // V-14 (2026-05-21) — Tauri 1.2.4 reference frames 005/006/007/008
        // paint a bright green status dot OVER the badge slot on the hovered
        // / active / expanded zone. The dot fades in with hover_t and
        // covers the numeric count while the cursor is parked on the pill.
        // Position: centered on badge rect, diameter ~ badge height. Color:
        // `pal.accent_green` (dark theme #22C55E). The fill rounded-rect uses
        // half-height radius to render a perfect circle. The count text is
        // still painted underneath; the dot just overlays it on hover.
        if hover_t > 0.0 {
            let dot_size = layout.badge.height.min(layout.badge.width);
            let dot_rect = bento_nano_style::Rect {
                x: layout.badge.x + (layout.badge.width - dot_size) * 0.5,
                y: layout.badge.y + (layout.badge.height - dot_size) * 0.5,
                width: dot_size,
                height: dot_size,
            };
            let dot_color = Color {
                a: pal.accent_green.a * hover_t.clamp(0.0, 1.0),
                ..pal.accent_green
            };
            self.fill_rounded_rect(
                dot_rect,
                dot_color,
                BorderRadius::all(dot_size * 0.5),
            )?;
        }
        let _ = (anim_now_ms, press_t);
        Ok(())
    }

    /// Wave G2 — paint the in-flight capsule morph. `morph = 0` reproduces
    /// the collapsed pill chrome, `morph = 1` reproduces the expanded zone
    /// surface; values in between paint the lerped rect at lerped corner
    /// radius + lerped fill alpha. Glyph + label + count badge fade in
    /// proportional to `morph` so the transient frame doesn't show truncated
    /// text. Allocation-free hot-path per spec §10.
    ///
    /// Matches the sibling `draw_zone_pill` arity allowance: the inputs
    /// (zone / layout / expanded rect / two independently-eased animation
    /// channels / theme accent / palette) are all genuinely distinct paint
    /// data — `morph` rides the 0.5 s easeOutBack size curve and `color_t`
    /// rides the SEPARATE 0.3 s CSS-`ease` color curve, so neither can be
    /// derived from the other locally.
    #[allow(clippy::too_many_arguments)]
    fn draw_zone_pill_morph(
        &mut self,
        zone: &Zone,
        pill_layout: &ZonePillLayout,
        expanded_rect: bento_nano_style::Rect,
        morph: f32,
        color_t: f32,
        theme_base_accent: Option<&str>,
        pal: bento_nano_style::tokens::PaletteTauri,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — live theme palette passed in by `draw_zones` (§10).
        use bento_nano_style::tokens::{ACRYLIC_FALLBACK, RADIUS, SHADOW};
        // M3 — `morph` may exceed 1.0 (easeOutBack ~10% overshoot). Geometry
        // (rect + radius) consumes the RAW value so the bulge is visible; the
        // shadow / fade interpolations use the [0,1] clamped value so the
        // chrome never over-saturates during the overshoot frame.
        // B2 — `color_t` is the SEPARATE 0.3s CSS-`ease` color channel (already
        // direction-resolved by the caller); the title-alpha rides it, NOT the
        // 500ms back curve.
        let morph_clamped = morph.clamp(0.0, 1.0);
        let pill_rect = pill_layout.rect;
        let rect = zone_pill_geometry::morph_pill_to_rect(pill_rect, expanded_rect, morph);
        // Capsule radius → expanded surface radius (RADIUS.expanded = 16 px,
        // matches the legacy zone chrome rounding). M2② — the morph START
        // radius reads the pill layout's OWN per-shape radius
        // (`pill_layout.radius`, resolved from `zone.capsule_shape`) instead of
        // the hardcoded `RADIUS.capsule`, so a rounded/minimal/circle capsule
        // uncurls from the radius it was actually painted at (no radius pop at
        // morph t=0) and stays consistent with the collapsed pill.
        let radius_px = zone_pill_geometry::morph_pill_radius(
            pill_layout.radius.top_left,
            RADIUS.expanded,
            morph,
        );
        let border_radius = BorderRadius::all(radius_px);
        let inv = 1.0 - morph_clamped;
        // Shadow band — pill drop offsets shrink linearly toward 0 as we
        // approach the expanded body (which carries its own legacy chrome).
        let shadow_outer = bento_nano_style::Rect {
            x: rect.x,
            y: rect.y + zone_pill_geometry::PILL_SHADOW_OUTER_DY * inv,
            width: rect.width,
            height: rect.height,
        };
        let shadow_inner = bento_nano_style::Rect {
            x: rect.x,
            y: rect.y + zone_pill_geometry::PILL_SHADOW_INNER_DY * inv,
            width: rect.width,
            height: rect.height,
        };
        // M6c — the `cyberpunk` neon bloom during the capsule<->panel morph,
        // painted UNDER the shadow band + surface fill. The glow lerps from the
        // collapsed (`.bento-zone`) layers to the expanded (`.bento-zone-expanded`)
        // layers by the clamped morph fraction so the bloom grows in lockstep
        // with the surface, with no pop at either endpoint (§10: stack-`f32`
        // lerp, 2 grown fills).
        if let bento_nano_style::tokens::EffectTauri::Neon(n) = effect {
            let morph_layers = [
                lerp_neon_layer(n.collapsed[0], n.expanded[0], morph_clamped),
                lerp_neon_layer(n.collapsed[1], n.expanded[1], morph_clamped),
            ];
            self.draw_neon_glow(rect, morph_layers, border_radius)?;
        }
        // M6b — `SHADOW.zen` is now a 2-layer `ShadowStack`; `.outer()`/`.inner()`
        // recover the pre-M6b `zen`/`zen_inner` single layers byte-for-byte.
        self.fill_rounded_rect(shadow_outer, SHADOW.zen.outer().color, border_radius)?;
        self.fill_rounded_rect(shadow_inner, SHADOW.zen.inner().color, border_radius)?;
        self.fill_rounded_rect(rect, ACRYLIC_FALLBACK, border_radius)?;
        // B2 (2026-05-29): Tauri's 0.3s `background`/`border-color` transition
        // is a visual no-op here: the collapsed pill and the expanded panel
        // share `surface_zen` (see `draw_zone_pill` line ~1848 and the expanded
        // chrome), and the morph path paints no border stroke — there is no
        // pill-vs-panel COLOR delta to crossfade, so the fill stays flat. The
        // 300ms `ease` channel (`color_t`) is therefore expressed only through
        // the title-alpha below, where it is genuinely visible.
        self.fill_rounded_rect(rect, pal.surface_zen, border_radius)?;
        // Accent stripe (matches expanded chrome accent dot above the icon
        // band — drawn at the top of the morph so the eye picks up the zone
        // identity even during the transition).
        let accent_hex = zone
            .accent_color
            .as_deref()
            .or(theme_base_accent);
        if let Some(accent) = accent_hex.and_then(parse_hex_color) {
            let accent_rect = bento_nano_style::Rect {
                x: rect.x + 8.0,
                y: rect.y + 4.0,
                width: (rect.width - 16.0).max(0.0),
                height: 3.0,
            };
            self.fill_rounded_rect(accent_rect, accent, BorderRadius::all(1.5))?;
        }
        // Title fades in along the morph. Use a thin top-band that scales
        // toward the expanded title area — anchored to the rect top-left so
        // it tracks the morph smoothly.
        let title_height = (12.0 + 6.0 * morph_clamped).min(rect.height - 8.0).max(8.0);
        let title_rect = bento_nano_style::Rect {
            x: rect.x + 10.0,
            y: rect.y + 6.0,
            width: (rect.width - 20.0).max(0.0),
            height: title_height,
        };
        // B2 — title-alpha rides the 0.3s CSS-`ease` color channel (`color_t`),
        // NOT the 0.5s easeOutBack size curve, so the text fade completes in
        // the first 300ms and matches Tauri's `background`/`border-color`
        // timeline rather than the springy size morph.
        let title_color = with_alpha(pal.text_primary, 0.6 + 0.4 * color_t);
        self.draw_text(zone.title.as_ref(), title_rect, title_color)?;
        Ok(())
    }

    // Geometric draw helper: the 8 params are independent paint primitives
    // (rect, fill, radius, text/icon colours, M3-A2 scale). Bundling them
    // into a struct adds indirection at the hot per-item call sites for no
    // real benefit — the conventional render-code shape, so allow it.
    #[allow(clippy::too_many_arguments)]
    fn draw_item_card(
        &mut self,
        item: &ZoneItem,
        base_rect: bento_nano_style::Rect,
        fill: Color,
        radius: BorderRadius,
        text: Color,
        icon_text: Color,
        scale: f32,
    ) -> Result<(), RenderError> {
        // M3-A2 (2026-05-29) — apply the `item_card::card_scale_for` hover/press
        // multiplier as a Tauri-style centred `transform: scale()`. The card
        // surface AND its inner icon/label inset offsets all inflate/deflate
        // about the card's CENTRE so the glyph + label stay centred (a CSS
        // transform scales the whole subtree, not just the box). `scale == 1.0`
        // (idle / drag-ghost) collapses to the original geometry exactly.
        let card_rect = animator::scale_rect_centered(base_rect, scale);
        self.fill_rounded_rect(card_rect, fill, radius)?;
        // Wave I2 — horizontally centre the icon glyph inside the card
        // (Tauri frame_010 reference). Vertical position keeps the existing
        // 6-DIP top inset (scaled with the card) so the label still sits in
        // the bottom band of the 80-DIP-tall card.
        let icon_side = item_icon::IconSize::Standard.container_px() * scale;
        let icon_rect = bento_nano_style::Rect {
            x: card_rect.x + ((card_rect.width - icon_side) * 0.5).max(0.0),
            y: card_rect.y + 6.0 * scale,
            width: icon_side,
            height: icon_side,
        };
        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect)? {
            // Wave I2 — prefer `draw_icon_glyph` so item icons that happen
            // to name a built-in `IconKind` paint as line-art. When the
            // path is not a known IconKind the helper falls through to
            // `draw_text`, where we hand it the extension-keyed emoji
            // fallback (the existing 1.x table) so unknown files still
            // render a recognisable glyph.
            let glyph = item_icon::fallback_emoji_for(item.path.as_ref());
            self.draw_icon_glyph(glyph.as_str(), icon_rect, icon_text)?;
        }
        let label_rect = bento_nano_style::Rect {
            x: card_rect.x + 4.0 * scale,
            y: card_rect.y + 44.0 * scale,
            width: (card_rect.width - 8.0 * scale).max(0.0),
            height: 28.0 * scale,
        };
        self.draw_text(item.name.as_ref(), label_rect, text)?;
        Ok(())
    }

    /// Draw an item icon bitmap if the backend cache has bytes for the item's
    /// icon hash. Returns `false` when fallback text should be used.
    fn draw_item_bitmap(
        &mut self,
        icon_hash: &str,
        rect: bento_nano_style::Rect,
    ) -> Result<bool, RenderError> {
        if icon_hash.is_empty() || self.icon_bitmap_failures.contains(icon_hash) {
            return Ok(false);
        }

        if !self.icon_bitmaps.contains_key(icon_hash) {
            let Some(cache) = bento_nano_backend::icon::cache_handle() else {
                return Ok(false);
            };
            let Some(bytes) = cache.get(icon_hash) else {
                let _ = self.icon_bitmap_failures.insert(icon_hash.to_owned());
                return Ok(false);
            };
            let Some(surface) = self.surface.as_ref() else {
                return Ok(false);
            };
            match d2d::bitmap_from_png_bytes(&surface.ctx, bytes.as_ref()) {
                Ok(bitmap) => {
                    let _ = self.icon_bitmaps.insert(icon_hash.to_owned(), bitmap);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "bentodesk::render::icon",
                        %icon_hash,
                        error = %e,
                        "failed to decode cached icon bitmap; using fallback glyph"
                    );
                    let _ = self.icon_bitmap_failures.insert(icon_hash.to_owned());
                    return Ok(false);
                }
            }
        }

        let Some(bitmap) = self.icon_bitmaps.get(icon_hash).cloned() else {
            return Ok(false);
        };
        let d2d_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width,
            bottom: rect.y + rect.height,
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(false);
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, d2d_rect, 1.0)?;
        Ok(true)
    }

    fn draw_image_file(
        &mut self,
        path: &str,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        if path.is_empty()
            || rect.width <= 0.0
            || rect.height <= 0.0
            || self.image_file_failures.contains(path)
        {
            return Ok(());
        }

        if !self.image_file_bitmaps.contains_key(path) {
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::image",
                        %path,
                        %error,
                        "failed to read file-backed image widget"
                    );
                    let _ = self.image_file_failures.insert(path.to_owned());
                    return Ok(());
                }
            };
            if bytes.len() > IMAGE_WIDGET_MAX_BYTES {
                tracing::warn!(
                    target: "bentodesk::render::image",
                    %path,
                    bytes = bytes.len(),
                    "file-backed image widget exceeds decode budget"
                );
                let _ = self.image_file_failures.insert(path.to_owned());
                return Ok(());
            }
            let Some(surface) = self.surface.as_ref() else {
                return Ok(());
            };
            match d2d::bitmap_from_image_bytes(&surface.ctx, &bytes) {
                Ok(bitmap) => {
                    let _ = self.image_file_bitmaps.insert(path.to_owned(), bitmap);
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::image",
                        %path,
                        error = %error,
                        "failed to decode file-backed image widget"
                    );
                    let _ = self.image_file_failures.insert(path.to_owned());
                    return Ok(());
                }
            }
        }

        let Some(bitmap) = self.image_file_bitmaps.get(path).cloned() else {
            return Ok(());
        };
        let d2d_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width,
            bottom: rect.y + rect.height,
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, d2d_rect, 1.0)?;
        Ok(())
    }

    /// Dedicated entry point for the `WindowKind::Settings` HWND. The HWND
    /// has its own 800×600 viewport (vs the Main HWND's primary-monitor work
    /// area), so painting the entire main UI tree + zones underneath the
    /// modal scrim leaks Main-window geometry into the Settings frame and
    /// causes overlap (button rects positioned for the Main viewport land
    /// outside the Settings panel chrome). Render only the scrim + panel +
    /// any open sub-modals, keeping the Settings HWND's frame self-contained.
    fn draw_settings_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        self.draw_settings_panel(app)
    }

    /// Phase 2.1 Ruling C — draw the modal settings overlay. Triggered by
    /// `app.settings_open == true`. Three layers:
    ///   1. Full-viewport α=0.30 black scrim so the underlying UI fades.
    ///   2. Centred 320×200 rounded panel with translucent dark fill.
    ///   3. Title + real settings rows + close button.
    fn draw_settings_panel(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::settings_panel::{
            SETTINGS_PANEL_RADIUS, SETTINGS_PANEL_SHADOW_ALPHA, SETTINGS_PERF_ROW_COUNT,
            SETTINGS_RADIO_INNER_D, SETTINGS_RADIO_OUTER_D, SETTINGS_ROW_PAD_X,
            SETTINGS_SLIDER_THUMB_D, SETTINGS_SOURCE_ROW_VISIBLE_MAX, SETTINGS_TOP_TOGGLE_COUNT,
            SETTINGS_ZONE_DISPLAY_MODE_COUNT, settings_body_rect,
            settings_cancel_button_rect, settings_close_button_rect_m1,
            settings_crash_max_retries_row_rect, settings_crash_restart_row_rect,
            settings_crash_window_row_rect, settings_desktop_path_input_rect,
            settings_desktop_path_label_rect, settings_footer_rect, settings_header_rect,
            settings_hibernate_slider_rect, settings_hibernate_slider_row_rect,
            settings_language_chevron_rect, settings_language_chip_label_rect,
            settings_language_chip_rect, settings_language_row_rect,
            settings_performance_label_rect, settings_performance_slider_rect,
            settings_performance_slider_row_rect, settings_panel_rect_m1, settings_safe_start_row_rect,
            settings_save_button_rect, settings_source_row_rect,
            settings_sources_label_rect, settings_sources_refresh_button_rect,
            settings_sources_reserve_delta,
            settings_startup_high_priority_row_rect,
            settings_startup_label_rect, settings_startup_toggle_hit_rect,
            settings_stealth_buttons_row_rect, settings_stealth_error_block_rect,
            settings_stealth_label_rect, settings_stealth_mirror_row_rect,
            settings_stealth_onedrive_block_rect, settings_stealth_pill_rect,
            settings_stealth_reapply_button_rect, settings_stealth_refresh_button_rect,
            settings_stealth_retry_row_rect, settings_stealth_schema_row_rect,
            settings_stealth_status_row_rect, settings_stepper_minus_rect, settings_stepper_plus_rect,
            settings_stepper_value_rect, settings_top_toggle_hit_rect, settings_top_toggle_row_rect,
            settings_updater_auto_download_hit_rect, settings_updater_auto_download_row_rect,
            settings_updater_button_rect, settings_updater_buttons_row_rect,
            settings_updater_frequency_chip_rect, settings_updater_frequency_row_rect,
            settings_updater_label_rect, settings_updater_middle_block_rect,
            settings_updater_pill_rect, settings_updater_progress_track_rect,
            settings_updater_status_row_rect, settings_watch_label_rect, settings_watch_textarea_rect,
            settings_backup_actions_row_rect, settings_backup_create_button_rect,
            settings_backup_description_rect, settings_backup_entry_row_rect,
            settings_backup_label_rect, settings_backup_refresh_button_rect,
            settings_backup_restore_button_rect, settings_backup_status_rect,
            SETTINGS_BACKUP_ROW_VISIBLE_MAX, SettingsBodyFlags, UpdaterHeightKind,
            settings_zone_display_mode_picker_row_rect,
            settings_zone_display_mode_radio_inner_rect,
            settings_zone_display_mode_radio_label_rect,
            settings_zone_display_mode_radio_outer_rect,
        };
        use crate::state::{SettingsUpdaterStatus, ZoneDisplayMode};
        use crate::widgets::toggle_switch::toggle_switch_in_rect;
        // Round-2 M1 — Tauri 1.2.4 frame_060/065/070/075 dark redesign.
        //
        // Three layers paint in order:
        //   1. Full-viewport α=0.55 scrim so the underlying desk fades hard.
        //   2. Dark dialog card (400 × min(700, viewport.h-padding), radius 14).
        //   3. Sticky 48-DIP header + scrollable body + sticky 56-DIP footer.
        //
        // Body content for M1: 5 toggle rows + language chip row.
        // K1 modal-opener arms (keybindings/plugins/theme picker) remain alive
        // as orphan paint paths gated on their own `*_open` Cells. They never
        // fire from M1 hit-test but compile-clean per Ruling B.
        // M6a — read the live theme palette so the whole Settings paint (panel
        // / header / footer / labels / accent / track) re-skins with the
        // active theme. Bound once; `PaletteTauri: Copy` (§10).
        let palette = app.active_theme_tauri();
        // A-path + V-3 (TL Ruling 2026-05-21): no fullscreen scrim — Tauri
        // 1.2.4 baseline (frame_060) leaves the desktop wallpaper visible
        // around the floating 420×580 modal. Panel/header/footer surfaces
        // ride at FULL opacity (1.0) so nothing behind the aux HWND (e.g.
        // editor windows, other apps) bleeds through the panel chrome.
        // Only the panel margin + drop shadow ring are translucent — those
        // need to compose against the wallpaper. surface_dialog =
        // #0C0C12EB, surface_subtle base alphas are ignored at 1.0.
        let panel_bg = with_alpha(palette.surface_dialog, 1.0);
        let header_bg = with_alpha(palette.surface_dialog, 1.0);
        // Footer rides on the same surface_dialog as panel/header. surface_subtle
        // in PALETTE_DARK is #FFFFFF×0x08 — a white-overlay token meant for
        // small hover accents, NOT a card fill. Forcing it to 1.0 alpha would
        // render the entire footer band white (regression observed in
        // 05-v-fixes capture pre-fix).
        let footer_bg = with_alpha(palette.surface_dialog, 1.0);
        let title_color = palette.text_primary;
        let label_color = palette.text_secondary;
        let accent_on = palette.accent_blue;
        let track_off = with_alpha(palette.surface_subtle, 0.80);
        // V-4 (TL audit 2026-05-21): chip_bg backs every M2/M3 interactive
        // surface — language dropdown, source cards, path input, watch
        // textarea, num buttons, overlay version input, cancel button. Prior
        // value `surface_hover×1.0` resolved to #FFFFFF×1.0 (pure white) which
        // bled white panels across the modal (see 05-v-fixes capture pre-fix).
        // surface_expanded = #0C0C12D1 — same hue as panel_bg, slightly
        // lighter alpha so cards read as raised against the dialog surface.
        let chip_bg = with_alpha(palette.surface_expanded, 1.0);
        let chip_border = with_alpha(palette.border_zen, 0.60);
        let knob_color = bento_nano_style::Color::WHITE;
        let divider_color = with_alpha(palette.text_primary, 0.08);
        let panel_radius = bento_nano_style::BorderRadius::all(SETTINGS_PANEL_RADIUS);
        // M6b — per-theme card radius for the Settings chip surfaces.
        let chip_radius_tokens = app.active_theme_radius_tauri();
        let chip_radius = bento_nano_style::BorderRadius::all(chip_radius_tokens.card);
        let header_radius = bento_nano_style::BorderRadius {
            top_left: SETTINGS_PANEL_RADIUS,
            top_right: SETTINGS_PANEL_RADIUS,
            bottom_left: 0.0,
            bottom_right: 0.0,
        };
        let footer_radius = bento_nano_style::BorderRadius {
            top_left: 0.0,
            top_right: 0.0,
            bottom_left: SETTINGS_PANEL_RADIUS,
            bottom_right: SETTINGS_PANEL_RADIUS,
        };
        let btn_radius = bento_nano_style::BorderRadius::all(8.0);

        // RC-4 Gap 2 — derive a layout viewport from backbuffer + base_scale.
        let base_scale = self.base_scale.max(0.01);
        let viewport = bento_nano_style::Size {
            width: (self.width as f32 / base_scale).max(1.0),
            height: (self.height as f32 / base_scale).max(1.0),
        };

        // 1) Drop shadow — A-path 2026-05-21: no fullscreen scrim (Tauri
        // baseline frame_060 keeps the desktop wallpaper visible around the
        // panel). Instead paint an outer soft-black box 8 DIP each side of
        // the panel so it reads as a floating modal lifted off the desktop.
        // V-5 (TL Ruling 2026-05-21): alpha pinned to
        // `SETTINGS_PANEL_SHADOW_ALPHA` (0.15) — the hard-edged
        // `fill_rounded_rect` cannot reproduce Tauri's gaussian falloff, so a
        // high alpha (pre-fix 0.45) reads as a "mask ring" against the
        // wallpaper. 0.15 keeps the lifted cue without the halo.
        let panel = settings_panel_rect_m1(viewport);
        let shadow_rect = bento_nano_style::Rect {
            x: panel.x - 8.0,
            y: panel.y - 8.0,
            width: panel.width + 16.0,
            height: panel.height + 16.0,
        };
        let shadow = with_alpha(
            bento_nano_style::Color::from_u8(0x00, 0x00, 0x00, 0xFF),
            SETTINGS_PANEL_SHADOW_ALPHA,
        );
        let shadow_radius = bento_nano_style::BorderRadius::all(SETTINGS_PANEL_RADIUS + 4.0);
        self.fill_rounded_rect(shadow_rect, shadow, shadow_radius)?;

        // 2) Panel card — dark, radius 14, sits on top of the shadow.
        self.fill_rounded_rect(panel, panel_bg, panel_radius)?;

        // 3) Header (sticky, 48 DIP) — title + close ×.
        let header = settings_header_rect(viewport);
        self.fill_rounded_rect(header, header_bg, header_radius)?;
        let title_rect = bento_nano_style::Rect {
            x: header.x + SETTINGS_ROW_PAD_X,
            y: header.y + (header.height - 20.0) * 0.5,
            width: header.width * 0.5,
            height: 20.0,
        };
        // M6c — settings panel title is the `h1`/`h2` heading; route through the
        // chromatic-aberration helper (editorial only; plain draw otherwise).
        self.draw_text_chromatic_title(
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_TITLE),
            title_rect,
            title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = settings_close_button_rect_m1(viewport);
        self.draw_text_no_wrap("×", close_rect, title_color)?;
        let header_hairline = bento_nano_style::Rect {
            x: header.x,
            y: header.bottom() - 1.0,
            width: header.width,
            height: 1.0,
        };
        self.fill_rounded_rect(header_hairline, divider_color, BorderRadius::ZERO)?;

        // 4) Body — paint rows scrolled by `app.scroll_offset_y`.
        //
        // M1b (S-02): clip the whole body band so partial rows at the top/bottom
        // edge are masked by the sticky header/footer instead of bleeding past
        // them (rows fully offscreen still early-skip via `row_visible`, but a
        // row straddling the edge now clips at the pixel boundary).
        //
        // CRITICAL — the body paint propagates with `?`, so a naive
        // `push; …?; pop` would leak the clip on the first D2D error and
        // corrupt the device context. We capture the body paint into a closure
        // result and ALWAYS run `pop_clip()` before propagating, keeping the
        // push/pop balanced across every early return. (No Drop guard: a
        // fallible pop in Drop is disallowed; this stays `?`-clean + panic-free.)
        let body = settings_body_rect(viewport);
        self.push_clip(body)?;
        let body_paint = (|| -> Result<(), RenderError> {
        let scroll = app.scroll_offset_y.get();

        // Helper: skip if row falls fully outside the body band.
        let row_visible = |row: Rect, body: Rect| -> bool {
            row.bottom() > body.y && row.y < body.bottom()
        };

        // Toggle row labels by index (0..=4). M1a 2026-05-29: row 4 text was
        // retargeted to Tauri "智能自动分组" (still id 116, const name
        // unchanged); row 5 swapped from the bespoke speed-mode id 117 to the
        // new Tauri "便携模式" id 141 (`SETTING_PORTABLE_MODE`).
        let toggle_labels: [u16; 5] = [
            bento_nano_style::i18n_zh_cn::ids::SETTING_DESKTOP_EMBED.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_AUTOSTART.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_SHOW_IN_TASKBAR.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_SMART_LAYOUT.0,
            bento_nano_style::i18n_zh_cn::ids::SETTING_PORTABLE_MODE.0,
        ];

        for index in 0..SETTINGS_TOP_TOGGLE_COUNT {
            let row = settings_top_toggle_row_rect(viewport, scroll, index);
            if !row_visible(row, body) {
                continue;
            }
            // Row label.
            let label_rect = bento_nano_style::Rect {
                x: row.x,
                y: row.y + (row.height - 16.0) * 0.5,
                width: row.width * 0.6,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::StringId(
                    toggle_labels[index as usize],
                )),
                label_rect,
                label_color,
            )?;
            // Toggle.
            let hit = settings_top_toggle_hit_rect(viewport, scroll, index);
            let on = match index {
                0 => app.setting_desktop_embed.get(),
                1 => app.setting_autostart.get(),
                2 => app.setting_show_in_taskbar.get(),
                3 => app.setting_smart_layout.get(),
                4 => app.setting_portable_mode.get(),
                _ => false,
            };
            let switch = toggle_switch_in_rect(hit);
            self.fill_rounded_rect(
                switch.track,
                if on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(on),
                knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }

        // Language row.
        let locale_row = settings_language_row_rect(viewport, scroll);
        if row_visible(locale_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: locale_row.x,
                y: locale_row.y + (locale_row.height - 16.0) * 0.5,
                width: locale_row.width * 0.45,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_LANGUAGE),
                label_rect,
                label_color,
            )?;
            let chip = settings_language_chip_rect(viewport, scroll);
            self.fill_rounded_rect(chip, chip_bg, chip_radius)?;
            let chip_hairline = bento_nano_style::Rect {
                x: chip.x,
                y: chip.y,
                width: chip.width,
                height: 1.0,
            };
            self.fill_rounded_rect(chip_hairline, chip_border, BorderRadius::ZERO)?;
            let locale_label =
                if bento_nano_style::current_locale_is(&bento_nano_style::EN_US) {
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_EN_US)
                } else {
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_ZH_CN)
                };
            self.draw_text_no_wrap(
                locale_label,
                settings_language_chip_label_rect(viewport, scroll),
                title_color,
            )?;
            self.draw_text_no_wrap(
                "▾",
                settings_language_chevron_rect(viewport, scroll),
                label_color,
            )?;
        }

        // α4 (Wave I-α, 2026-05-25) — zone-display-mode 3-radio picker.
        //
        // Tauri 1.2.4 baseline (`SettingsPanel.tsx:555-595`) paints a 3-radio
        // horizontal group (Hover / Always / Click) for the default zone
        // display mode. Wave H shipped the data path (enum + get/set +
        // SettingsHit::CycleZoneDisplayMode dispatch) but no UI — the row
        // index lived as orphan `#[allow(dead_code)]` per evidence row R2.
        // This block paints the row + 3 radios; the hit-tester in
        // `bento-nano-shell/src/ui.rs::settings_hit` and the dispatch arm in
        // `bento-nano-shell/src/main.rs` route clicks back into the same
        // `Command::SetSetting` path the cycle button used.
        let picker_row = settings_zone_display_mode_picker_row_rect(viewport, scroll);
        if row_visible(picker_row, body) {
            // Row label on the left half — matches the language row layout.
            let label_rect = bento_nano_style::Rect {
                x: picker_row.x,
                y: picker_row.y + (picker_row.height - 16.0) * 0.5,
                width: picker_row.width * 0.4,
                height: 16.0,
            };
            // R14 fix (2026-05-25) — caption uses dedicated picker-row label
            // StringId 140 ("默认显示模式" / "Default display mode"), bilingual
            // and unrelated to the per-radio mode names (77/78/79).
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTINGS_ZONE_DISPLAY_MODE_LABEL,
                ),
                label_rect,
                label_color,
            )?;
            let modes = [
                ZoneDisplayMode::Hover,
                ZoneDisplayMode::Always,
                ZoneDisplayMode::Click,
            ];
            let current = app.zone_display_mode.get();
            let radius_outer = BorderRadius::all(SETTINGS_RADIO_OUTER_D * 0.5);
            let radius_inner = BorderRadius::all(SETTINGS_RADIO_INNER_D * 0.5);
            for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
                let mode = modes[index as usize];
                let outer = settings_zone_display_mode_radio_outer_rect(
                    viewport, scroll, index,
                );
                // Selected radios use the accent hue; unselected keep the
                // chip_border tone. v1.3.0 SettingsPanel.tsx ring pattern:
                // fill the outer disc with ring_color, then carve out the
                // interior with the panel surface — leaves a 1-DIP ring on
                // ALL four edges (top/bottom/left/right) at once, no
                // per-edge band stitching (R14 fix — prior 2-band version
                // read as `(== ==)` not `○`).
                let ring_color = if mode == current {
                    accent_on
                } else {
                    chip_border
                };
                self.fill_rounded_rect(outer, ring_color, radius_outer)?;
                let ring_hairline: f32 = 1.0;
                let interior = bento_nano_style::Rect {
                    x: outer.x + ring_hairline,
                    y: outer.y + ring_hairline,
                    width: (outer.width - 2.0 * ring_hairline).max(0.0),
                    height: (outer.height - 2.0 * ring_hairline).max(0.0),
                };
                let radius_interior = BorderRadius::all(
                    (SETTINGS_RADIO_OUTER_D * 0.5 - ring_hairline).max(0.0),
                );
                self.fill_rounded_rect(interior, chip_bg, radius_interior)?;
                if mode == current {
                    let inner = settings_zone_display_mode_radio_inner_rect(
                        viewport, scroll, index,
                    );
                    self.fill_rounded_rect(inner, accent_on, radius_inner)?;
                }
                // Radio label — bilingual via StringId 77/78/79 (R14 fix —
                // prior `mode.label()` returned English-only literals).
                let label_id = match mode {
                    ZoneDisplayMode::Hover => {
                        bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_HOVER
                    }
                    ZoneDisplayMode::Always => {
                        bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_ALWAYS
                    }
                    ZoneDisplayMode::Click => {
                        bento_nano_style::i18n_zh_cn::ids::ZONE_MODE_CLICK
                    }
                };
                let label = settings_zone_display_mode_radio_label_rect(
                    viewport, scroll, index,
                );
                self.draw_text_no_wrap(
                    bento_nano_style::t(label_id),
                    label,
                    title_color,
                )?;
            }
        }

        // ── Round-2 M2 sections ──────────────────────────────────────────

        // 桌面源 label (M1i fidelity — Tauri `.settings-row__label` ABOVE the
        // `.desktop-source-list`; refresh button is now the list's LAST child,
        // painted after the cards below, `SettingsPanel.tsx:317-361`).
        let source_count = app.desktop_sources.borrow().len();
        let sources_label = settings_sources_label_rect(viewport, scroll);
        if row_visible(sources_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_SOURCES),
                sources_label,
                label_color,
            )?;
        }

        // M1i fidelity — `.desktop-source-card` geometry/typography translated
        // 1:1 from `SettingsPanel.css:665-770`:
        //   card  : radius 8, bg white@4%, border 1px solid border_zen,
        //           padding 8/10, icon→body gap 10, inter-card gap 6
        //   icon  : 28×28 CIRCLE, white initial, font 12 semibold, per-kind bg
        //           @0.75 (User=blue Public=green OneDrive=sky Custom=purple)
        //   body  : label 13 medium text_primary, path 11 MONOSPACE text_muted
        //           with ellipsis trim, internal gap 2
        //   badge : green@0.18 bg, accent_green text, 9px semibold UPPERCASE,
        //           padding 2/8, radius 10, AUTO width right-aligned, centred
        // The list snapshot is owned by AppState and refreshed on open /
        // RefreshDesktopSources, never built per-frame (architecture §10).
        const CARD_PAD_X: f32 = 10.0;
        const ICON_SIZE: f32 = 28.0;
        const ICON_BODY_GAP: f32 = 10.0;
        const BODY_GAP: f32 = 2.0;
        const LABEL_LINE_H: f32 = 16.0;
        const PATH_LINE_H: f32 = 14.0;
        let card_radius = bento_nano_style::BorderRadius::all(8.0);
        let card_bg = bento_nano_style::Color::from_u8(0xFF, 0xFF, 0xFF, 0x0A); // white @ ~4%
        let card_border = palette.border_zen;
        let sources = app.desktop_sources.borrow();
        let visible_sources = sources.len().min(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
        for index in 0..visible_sources {
            let row = settings_source_row_rect(viewport, scroll, index as u8);
            if !row_visible(row, body) {
                continue;
            }
            let (kind, path_text, watched) = &sources[index];
            // Card surface + 1px hairline border (Tauri `border: 1px solid
            // var(--border-zen)` — the nano card previously had NO stroke).
            self.fill_rounded_rect(row, card_bg, card_radius)?;
            self.stroke_rounded_rect(row, card_border, card_radius, 1.0)?;
            // 28×28 CIRCLE with the kind initial (was a 24×24 rounded square).
            // A square fill_rounded_rect with radius = half-side is a true
            // circle. Per-kind LITERAL rgba @0.75 (palette.accent_purple is
            // 139,92,246 — NOT the 168,85,247 Tauri purple — so Custom uses a
            // literal; OneDrive's sky 14,165,233 has no palette token either).
            let icon_rect = bento_nano_style::Rect {
                x: row.x + CARD_PAD_X,
                y: row.y + (row.height - ICON_SIZE) * 0.5,
                width: ICON_SIZE,
                height: ICON_SIZE,
            };
            let (icon_bg, icon_glyph, kind_label_id) = match kind {
                bento_nano_backend::desktop_sources::DesktopSourceKind::User => (
                    bento_nano_style::Color::from_u8(59, 130, 246, 191), // 0.75
                    "U",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_PRIMARY_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::Public => (
                    bento_nano_style::Color::from_u8(34, 197, 94, 191),
                    "P",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_PUBLIC_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::OneDrive => (
                    bento_nano_style::Color::from_u8(14, 165, 233, 191), // sky (fixed)
                    "O",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_ONEDRIVE_LABEL,
                ),
                bento_nano_backend::desktop_sources::DesktopSourceKind::Custom => (
                    bento_nano_style::Color::from_u8(168, 85, 247, 191), // purple (fixed)
                    "C",
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_CUSTOM_LABEL,
                ),
            };
            self.fill_rounded_rect(
                icon_rect,
                icon_bg,
                bento_nano_style::BorderRadius::all(ICON_SIZE * 0.5),
            )?;
            self.draw_text_centered(
                icon_glyph,
                icon_rect,
                bento_nano_style::Color::WHITE,
                12.0,
                600,
            )?;
            // Body column (flex:1, gap 2): label line on top, path line below,
            // the pair vertically centred against the icon.
            let body_x = icon_rect.right() + ICON_BODY_GAP;
            // Reserve room on the right for the badge so the path never runs
            // under it (Tauri's flex `min-width:0` body shrinks for the badge).
            let badge_reserve: f32 = if *watched { 76.0 } else { 0.0 };
            let body_w = (row.right() - CARD_PAD_X - badge_reserve - body_x).max(1.0);
            let block_h = LABEL_LINE_H + BODY_GAP + PATH_LINE_H;
            let body_top = row.y + (row.height - block_h) * 0.5;
            let label_rect = bento_nano_style::Rect {
                x: body_x,
                y: body_top,
                width: body_w,
                height: LABEL_LINE_H,
            };
            self.draw_text_with_style(
                bento_nano_style::t(kind_label_id),
                label_rect,
                title_color,
                13.0,
                500,
                1.0,
            )?;
            // Path line — REAL resolved path, MONOSPACE, ellipsis-trimmed.
            let path_rect = bento_nano_style::Rect {
                x: body_x,
                y: body_top + LABEL_LINE_H + BODY_GAP,
                width: body_w,
                height: PATH_LINE_H,
            };
            self.draw_text_monospace_ellipsis(
                path_text.as_str(),
                path_rect,
                palette.text_muted,
                11.0,
            )?;
            // Watched badge — translucent green tint, accent_green text, auto
            // width right-aligned, vertically centred (was a solid-green fill
            // with WHITE text in a fixed 56×22 rect).
            if *watched {
                let badge_text = bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SOURCE_WATCHED_BADGE,
                );
                let badge_upper = badge_text.to_uppercase();
                // Auto width: shrink-to-fit the text + 8px padding each side.
                // CJK glyphs ≈ font_size wide, Latin ≈ font_size*0.62, plus the
                // 0.8px letter-spacing Tauri applies per glyph.
                const BADGE_FONT: f32 = 9.0;
                const BADGE_PAD_X: f32 = 8.0;
                const BADGE_LETTER_SPACING: f32 = 0.8;
                let glyph_count = badge_upper.chars().count() as f32;
                let text_w: f32 = badge_upper
                    .chars()
                    .map(|c| {
                        if (c as u32) > 0x2E80 {
                            BADGE_FONT
                        } else {
                            BADGE_FONT * 0.62
                        }
                    })
                    .sum::<f32>()
                    + BADGE_LETTER_SPACING * glyph_count;
                let badge_w = text_w + BADGE_PAD_X * 2.0;
                let badge_h: f32 = 16.0; // 2px pad + ~12 line box
                let badge_rect = bento_nano_style::Rect {
                    x: row.right() - CARD_PAD_X - badge_w,
                    y: row.y + (row.height - badge_h) * 0.5,
                    width: badge_w,
                    height: badge_h,
                };
                let badge_bg = with_alpha(palette.accent_green, 0.18);
                self.fill_rounded_rect(
                    badge_rect,
                    badge_bg,
                    bento_nano_style::BorderRadius::all(10.0),
                )?;
                self.draw_text_centered(
                    badge_upper.as_str(),
                    badge_rect,
                    palette.accent_green,
                    BADGE_FONT,
                    600,
                )?;
            }
        }
        drop(sources);

        // M1i fidelity — empty `.desktop-source-empty` placeholder (italic,
        // 11px, text_muted) when no desktop sources resolve. nano's refresh is
        // synchronous (no async loading frame), so Tauri's "…" loading glyph is
        // N/A by construction — there is never a loading state to paint.
        if visible_sources == 0 {
            let label = settings_sources_label_rect(viewport, scroll);
            let empty_rect = bento_nano_style::Rect {
                x: label.x + 4.0,
                y: label.bottom() + 6.0,
                width: (label.width - 8.0).max(1.0),
                height: 12.0,
            };
            if row_visible(empty_rect, body) {
                // No italic system face is loaded; the muted tone + xs size
                // reads as the de-emphasised placeholder Tauri renders italic.
                self.draw_text_with_style(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::SOURCE_EMPTY_PLACEHOLDER,
                    ),
                    empty_rect,
                    palette.text_muted,
                    11.0,
                    400,
                    1.0,
                )?;
            }
        }

        // M1i fidelity — refresh (`↻`) button: LAST child of the list,
        // right-anchored BELOW the cards / placeholder (`align-self:flex-end`).
        // Secondary-button style: chip_bg fill, radius, centred 14px glyph.
        let refresh_btn = settings_sources_refresh_button_rect(viewport, scroll, source_count);
        if row_visible(refresh_btn, body) {
            self.fill_rounded_rect(
                refresh_btn,
                chip_bg,
                bento_nano_style::BorderRadius::all(6.0),
            )?;
            self.stroke_rounded_rect(
                refresh_btn,
                chip_border,
                bento_nano_style::BorderRadius::all(6.0),
                1.0,
            )?;
            // U+21BB CLOCKWISE OPEN CIRCLE ARROW — the refresh glyph, centred.
            self.draw_text_centered("\u{21BB}", refresh_btn, title_color, 14.0, 400)?;
        }

        // 桌面路径 label + input (reflows below the live source stack).
        let path_label = settings_desktop_path_label_rect(viewport, scroll, source_count);
        if row_visible(path_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_PATH),
                path_label,
                label_color,
            )?;
        }
        // Input/textarea boxes keep the radius-10 surface the M2 layout shipped.
        let input_box_radius = bento_nano_style::BorderRadius::all(10.0);
        let path_input = settings_desktop_path_input_rect(viewport, scroll, source_count);
        if row_visible(path_input, body) {
            self.fill_rounded_rect(path_input, chip_bg, input_box_radius)?;
            let path_text = app.desktop_path_draft.borrow();
            let text_rect = bento_nano_style::Rect {
                x: path_input.x + 12.0,
                y: path_input.y + (path_input.height - 16.0) * 0.5,
                width: (path_input.width - 24.0).max(0.0),
                height: 16.0,
            };
            self.draw_text_no_wrap(path_text.as_str(), text_rect, title_color)?;
            drop(path_text);
        }

        // 监控值 label + textarea (reflows below the live source stack).
        let watch_label = settings_watch_label_rect(viewport, scroll, source_count);
        if row_visible(watch_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SECTION_WATCH_VALUES),
                watch_label,
                label_color,
            )?;
        }
        let watch_area = settings_watch_textarea_rect(viewport, scroll, source_count);
        if row_visible(watch_area, body) {
            self.fill_rounded_rect(watch_area, chip_bg, input_box_radius)?;
            let watch_text = app.watch_paths_draft.borrow();
            if watch_text.is_empty() {
                // Hint placeholder.
                let hint_rect = bento_nano_style::Rect {
                    x: watch_area.x + 12.0,
                    y: watch_area.y + 10.0,
                    width: (watch_area.width - 24.0).max(0.0),
                    height: 16.0,
                };
                self.draw_text(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::WATCH_HINT_LINE_EACH,
                    ),
                    hint_rect,
                    label_color,
                )?;
            } else {
                let text_rect = bento_nano_style::Rect {
                    x: watch_area.x + 12.0,
                    y: watch_area.y + 10.0,
                    width: (watch_area.width - 24.0).max(0.0),
                    height: (watch_area.height - 20.0).max(0.0),
                };
                self.draw_text(watch_text.as_str(), text_rect, title_color)?;
            }
            drop(watch_text);
        }

        // ── M1d sections — Performance §5 + Startup management §6 ────────
        //
        // Replaces the deleted bespoke 高级 / 未来集成验证 blocks with the two
        // genuine Tauri sections (`SettingsPanel.tsx:601-698`). Performance =
        // 3 SliderRows (no conditionals). Startup = 2 toggles + 2 conditional
        // steppers (crash_restart) + 1 toggle + 1 conditional slider
        // (hibernation). The hit-tester in `bento-nano-shell::ui::settings_hit`
        // + the dispatch arms in `main.rs` route every control fully through
        // paint→hit→dispatch→persist→snapshot.
        let num_btn_radius = bento_nano_style::BorderRadius::all(6.0);
        let slider_track_radius = bento_nano_style::BorderRadius::all(2.0);
        let slider_thumb_radius = bento_nano_style::BorderRadius::all(SETTINGS_SLIDER_THUMB_D * 0.5);

        // Read the two gating bools once so paint matches geometry exactly.
        let crash_restart_on = app.crash_restart_enabled.get();
        let safe_start_on = app.safe_start_after_hibernation.get();

        // M1i fidelity — single-base-offset reflow. The Performance §5 group and
        // EVERY section below it (Startup/Stealth/Updater/Backup/Plugins) root
        // at `settings_perf_origin_y_offset`, which is pinned at the fixed
        // 4-card source reserve. Folding the live reserve delta into `scroll`
        // shifts the whole lower body UP by the height of the missing source
        // cards (Tauri's flex column) — shadowing `scroll` here propagates the
        // shift to all perf-and-below geometry fns without touching their
        // signatures. The hit-tester applies the identical fold (`ui.rs`).
        let scroll = scroll + settings_sources_reserve_delta(source_count);

        // Performance group title.
        let perf_label = settings_performance_label_rect(viewport, scroll);
        if row_visible(perf_label, body) {
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_PERFORMANCE,
                ),
                perf_label,
                label_color,
            )?;
        }

        // Performance SliderRows. Each: label + tabular "{v}{unit}" on the top
        // line, full-width track band + filled segment + thumb on the lower
        // line (matches Tauri `.slider-row`, `SettingsPanel.tsx:848-871`).
        let perf_rows: [(u16, i32, i32, &'static str); 3] = [
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_EXPAND_DELAY.0,
                crate::state::EXPAND_DELAY_MIN_MS,
                crate::state::EXPAND_DELAY_MAX_MS,
                "ms",
            ),
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_COLLAPSE_DELAY.0,
                crate::state::COLLAPSE_DELAY_MIN_MS,
                crate::state::COLLAPSE_DELAY_MAX_MS,
                "ms",
            ),
            (
                bento_nano_style::i18n_zh_cn::ids::SETTING_ICON_CACHE_SIZE.0,
                crate::state::ICON_CACHE_MIN,
                crate::state::ICON_CACHE_MAX,
                "",
            ),
        ];
        for index in 0..SETTINGS_PERF_ROW_COUNT {
            let row = settings_performance_slider_row_rect(viewport, scroll, index);
            if !row_visible(row, body) {
                continue;
            }
            let (label_id, min, max, unit) = perf_rows[index as usize];
            let raw = match index {
                0 => app.expand_delay_ms.get(),
                1 => app.collapse_delay_ms.get(),
                _ => app.icon_cache_size.get(),
            };
            let value = raw.clamp(min, max);
            // Top line: label (left) + value (right, tabular).
            let label_rect = bento_nano_style::Rect {
                x: row.x,
                y: row.y + 4.0,
                width: row.width * 0.6,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::StringId(label_id)),
                label_rect,
                label_color,
            )?;
            let value_text = if unit.is_empty() {
                smol_str::SmolStr::new(value.to_string())
            } else {
                smol_str::SmolStr::new(format!("{value}{unit}"))
            };
            let value_rect = bento_nano_style::Rect {
                x: row.x + row.width * 0.6,
                y: row.y + 4.0,
                width: row.width * 0.4,
                height: 16.0,
            };
            self.draw_text_no_wrap(value_text.as_str(), value_rect, title_color)?;
            // Lower line: slider track + filled segment + thumb.
            let track = settings_performance_slider_rect(viewport, scroll, index);
            let track_band = bento_nano_style::Rect {
                x: track.x,
                y: track.y + (track.height - 4.0) * 0.5,
                width: track.width,
                height: 4.0,
            };
            self.fill_rounded_rect(track_band, track_off, slider_track_radius)?;
            let span = (max - min).max(1) as f32;
            let frac = ((value - min) as f32 / span).clamp(0.0, 1.0);
            let filled = bento_nano_style::Rect {
                x: track_band.x,
                y: track_band.y,
                width: track_band.width * frac,
                height: track_band.height,
            };
            self.fill_rounded_rect(filled, accent_on, slider_track_radius)?;
            let thumb_d = track.height;
            let thumb = bento_nano_style::Rect {
                x: track.x + track.width * frac - thumb_d * 0.5,
                y: track.y,
                width: thumb_d,
                height: thumb_d,
            };
            self.fill_rounded_rect(thumb, knob_color, slider_thumb_radius)?;
        }

        // Startup management group title.
        let startup_label = settings_startup_label_rect(viewport, scroll);
        if row_visible(startup_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_STARTUP),
                startup_label,
                label_color,
            )?;
        }

        // Reusable toggle-row paint: label (left) + desc caption + rocker.
        // Returns the toggle hit-box so the caller can drop it (unused here).
        // We inline rather than closure to keep `self` borrows simple.
        // Row 0 — 高优先级启动 (always).
        let high_row = settings_startup_high_priority_row_rect(viewport, scroll);
        if row_visible(high_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: high_row.x,
                y: high_row.y + (high_row.height - 16.0) * 0.5,
                width: high_row.width * 0.6,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_STARTUP_HIGH_PRIORITY,
                ),
                label_rect,
                label_color,
            )?;
            let on = app.startup_high_priority.get();
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(high_row));
            self.fill_rounded_rect(
                switch.track,
                if on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(on),
                knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        // Row 0 desc caption.
        let high_desc = bento_nano_style::Rect {
            x: high_row.x,
            y: high_row.bottom() + 1.0,
            width: high_row.width,
            height: 14.0,
        };
        if row_visible(high_desc, body) {
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_STARTUP_HIGH_PRIORITY_DESC,
                ),
                high_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Row 1 — 崩溃自动重启 (always, gates the steppers).
        let crash_row = settings_crash_restart_row_rect(viewport, scroll);
        if row_visible(crash_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: crash_row.x,
                y: crash_row.y + (crash_row.height - 16.0) * 0.5,
                width: crash_row.width * 0.6,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART),
                label_rect,
                label_color,
            )?;
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(crash_row));
            self.fill_rounded_rect(
                switch.track,
                if crash_restart_on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(crash_restart_on),
                knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        let crash_desc = bento_nano_style::Rect {
            x: crash_row.x,
            y: crash_row.bottom() + 1.0,
            width: crash_row.width,
            height: 14.0,
        };
        if row_visible(crash_desc, body) {
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART_DESC,
                ),
                crash_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Rows 2/3 — crash steppers, ONLY when crash_restart_on. Each: label
        // (left) + a "− value +" stepper (right). The − / + glyphs are drawn
        // so the stepper reads as interactive (Tauri uses a native number
        // input; nano keeps the stepper chrome).
        if crash_restart_on {
            let stepper_rows: [(u16, Rect, i32); 2] = [
                (
                    bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_MAX_RETRIES.0,
                    settings_crash_max_retries_row_rect(viewport, scroll),
                    app.crash_max_retries.get(),
                ),
                (
                    bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_WINDOW_SECS.0,
                    settings_crash_window_row_rect(viewport, scroll),
                    app.crash_window_secs.get(),
                ),
            ];
            for (label_id, row, value) in stepper_rows {
                if !row_visible(row, body) {
                    continue;
                }
                let label_rect = bento_nano_style::Rect {
                    x: row.x,
                    y: row.y + (row.height - 16.0) * 0.5,
                    width: row.width * 0.6,
                    height: 16.0,
                };
                self.draw_text(
                    bento_nano_style::t(bento_nano_style::StringId(label_id)),
                    label_rect,
                    label_color,
                )?;
                let minus = settings_stepper_minus_rect(row);
                let val_rect = settings_stepper_value_rect(row);
                let plus = settings_stepper_plus_rect(row);
                self.fill_rounded_rect(minus, chip_bg, num_btn_radius)?;
                self.draw_text_no_wrap("−", minus, title_color)?;
                let buf = smol_str::SmolStr::new(value.to_string());
                self.draw_text_no_wrap(buf.as_str(), val_rect, title_color)?;
                self.fill_rounded_rect(plus, chip_bg, num_btn_radius)?;
                self.draw_text_no_wrap("+", plus, title_color)?;
            }
        }

        // Row 4 — 休眠安全恢复 (always, gates the hibernate slider). Its Y
        // depends on whether the crash steppers are present.
        let safe_row = settings_safe_start_row_rect(viewport, scroll, crash_restart_on);
        if row_visible(safe_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: safe_row.x,
                y: safe_row.y + (safe_row.height - 16.0) * 0.5,
                width: safe_row.width * 0.6,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_SAFE_START_HIBERNATION,
                ),
                label_rect,
                label_color,
            )?;
            let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(safe_row));
            self.fill_rounded_rect(
                switch.track,
                if safe_start_on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(safe_start_on),
                knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        let safe_desc = bento_nano_style::Rect {
            x: safe_row.x,
            y: safe_row.bottom() + 1.0,
            width: safe_row.width,
            height: 14.0,
        };
        if row_visible(safe_desc, body) {
            self.draw_text(
                bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::SETTING_SAFE_START_HIBERNATION_DESC,
                ),
                safe_desc,
                with_alpha(label_color, 0.7),
            )?;
        }

        // Row 5 — 恢复延迟 SliderRow, ONLY when safe_start_on.
        if safe_start_on {
            let row = settings_hibernate_slider_row_rect(viewport, scroll, crash_restart_on);
            if row_visible(row, body) {
                let value = app
                    .hibernate_resume_delay_ms
                    .get()
                    .clamp(
                        crate::state::HIBERNATE_DELAY_MIN_MS,
                        crate::state::HIBERNATE_DELAY_MAX_MS,
                    );
                let label_rect = bento_nano_style::Rect {
                    x: row.x,
                    y: row.y + 4.0,
                    width: row.width * 0.6,
                    height: 16.0,
                };
                self.draw_text(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::SETTING_HIBERNATE_DELAY,
                    ),
                    label_rect,
                    label_color,
                )?;
                let value_text = smol_str::SmolStr::new(format!("{value}ms"));
                let value_rect = bento_nano_style::Rect {
                    x: row.x + row.width * 0.6,
                    y: row.y + 4.0,
                    width: row.width * 0.4,
                    height: 16.0,
                };
                self.draw_text_no_wrap(value_text.as_str(), value_rect, title_color)?;
                let track = settings_hibernate_slider_rect(viewport, scroll, crash_restart_on);
                let track_band = bento_nano_style::Rect {
                    x: track.x,
                    y: track.y + (track.height - 4.0) * 0.5,
                    width: track.width,
                    height: 4.0,
                };
                self.fill_rounded_rect(track_band, track_off, slider_track_radius)?;
                let span = (crate::state::HIBERNATE_DELAY_MAX_MS
                    - crate::state::HIBERNATE_DELAY_MIN_MS)
                    .max(1) as f32;
                let frac =
                    ((value - crate::state::HIBERNATE_DELAY_MIN_MS) as f32 / span).clamp(0.0, 1.0);
                let filled = bento_nano_style::Rect {
                    x: track_band.x,
                    y: track_band.y,
                    width: track_band.width * frac,
                    height: track_band.height,
                };
                self.fill_rounded_rect(filled, accent_on, slider_track_radius)?;
                let thumb_d = track.height;
                let thumb = bento_nano_style::Rect {
                    x: track.x + track.width * frac - thumb_d * 0.5,
                    y: track.y,
                    width: thumb_d,
                    height: thumb_d,
                };
                self.fill_rounded_rect(thumb, knob_color, slider_thumb_radius)?;
            }
        }

        // ── M1e — Stealth §7 card (`StealthModeCard.tsx`) ───────────────
        //
        // Sits after Startup in the Tauri body order. Reads the cached
        // `app.stealth_status` snapshot (refreshed by the shell on open +
        // Refresh/Reapply). Status pill kind/label derive via
        // `StatusLevel::from_status` (1:1 with Tauri `deriveLevel`). The
        // retry/error/OneDrive rows are conditional; the geometry helpers take
        // the same `has_retry`/`has_error` flags so paint matches hit-test.
        use crate::business::settings::stealth_mode_card::StatusLevel;
        let stealth_label = settings_stealth_label_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
        );
        if row_visible(stealth_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_GROUP_TITLE),
                stealth_label,
                label_color,
            )?;
        }
        // Snapshot the conditional flags + cloned fields out of the RefCell so
        // the borrow does not span the fallible paint calls below.
        let stealth_snapshot = app.stealth_status.borrow().clone();
        let (has_retry, has_error) = match &stealth_snapshot {
            Some(s) => (s.retry_count > 0, s.last_error.is_some()),
            None => (false, false),
        };
        // Helper to paint a `label | value` row (label left, value right).
        // Inlined per-row below to keep `self` borrows simple.
        let stealth_value_x_frac = 0.5_f32;
        // Row 0 — status (label + colored pill), always shown.
        let status_row = settings_stealth_status_row_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
        );
        if row_visible(status_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: status_row.x,
                y: status_row.y + (status_row.height - 16.0) * 0.5,
                width: status_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_LABEL),
                label_rect,
                label_color,
            )?;
            let pill = settings_stealth_pill_rect(status_row);
            let pill_radius = bento_nano_style::BorderRadius::all(pill.height * 0.5);
            // Pill kind → colour. Applied = green (#36C86B, matching the
            // source-card pill tone), Pending = amber (#F59E0B per the Tauri
            // `--accent-amber`), Failed = red (accent_red token).
            let (pill_bg, pill_label_id) = match stealth_snapshot.as_ref() {
                Some(s) => {
                    let level = StatusLevel::from_status(s);
                    let bg = match level {
                        StatusLevel::Applied => with_alpha(
                            bento_nano_style::Color::from_u8(0x36, 0xC8, 0x6B, 0xFF),
                            0.90,
                        ),
                        StatusLevel::Pending => with_alpha(
                            bento_nano_style::Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF),
                            0.90,
                        ),
                        StatusLevel::Failed => with_alpha(palette.accent_red, 0.90),
                    };
                    (bg, level.label_id())
                }
                None => (
                    with_alpha(palette.surface_subtle, 0.85),
                    bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_PENDING,
                ),
            };
            self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(pill_label_id),
                pill,
                bento_nano_style::Color::WHITE,
            )?;
        }
        // Row 1 — schema version (label + value), always shown.
        let schema_row = settings_stealth_schema_row_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
        );
        if row_visible(schema_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: schema_row.x,
                y: schema_row.y + (schema_row.height - 16.0) * 0.5,
                width: schema_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_SCHEMA_VERSION),
                label_rect,
                label_color,
            )?;
            let value_rect = bento_nano_style::Rect {
                x: schema_row.x + schema_row.width * stealth_value_x_frac,
                y: label_rect.y,
                width: schema_row.width * (1.0 - stealth_value_x_frac),
                height: 16.0,
            };
            let schema_text = match stealth_snapshot.as_ref() {
                Some(s) => smol_str::SmolStr::new(s.schema_version.as_str()),
                None => smol_str::SmolStr::new_static("—"),
            };
            self.draw_text_no_wrap(schema_text.as_str(), value_rect, title_color)?;
        }
        // Row 2 — mirror health (label + 健康/异常), always shown.
        let mirror_row = settings_stealth_mirror_row_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
        );
        if row_visible(mirror_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: mirror_row.x,
                y: mirror_row.y + (mirror_row.height - 16.0) * 0.5,
                width: mirror_row.width * stealth_value_x_frac,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY),
                label_rect,
                label_color,
            )?;
            let value_rect = bento_nano_style::Rect {
                x: mirror_row.x + mirror_row.width * stealth_value_x_frac,
                y: label_rect.y,
                width: mirror_row.width * (1.0 - stealth_value_x_frac),
                height: 16.0,
            };
            let healthy = stealth_snapshot
                .as_ref()
                .map(|s| s.mirror_healthy)
                .unwrap_or(true);
            let mirror_id = if healthy {
                bento_nano_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY_YES
            } else {
                bento_nano_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY_NO
            };
            self.draw_text_no_wrap(
                bento_nano_style::t(mirror_id),
                value_rect,
                title_color,
            )?;
        }
        // Row 3 — retry count (label + value), ONLY when retry_count > 0.
        if has_retry {
            let retry_row = settings_stealth_retry_row_rect(
                viewport,
                scroll,
                crash_restart_on,
                safe_start_on,
            );
            if row_visible(retry_row, body) {
                let label_rect = bento_nano_style::Rect {
                    x: retry_row.x,
                    y: retry_row.y + (retry_row.height - 16.0) * 0.5,
                    width: retry_row.width * stealth_value_x_frac,
                    height: 16.0,
                };
                self.draw_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_RETRY_COUNT),
                    label_rect,
                    label_color,
                )?;
                let value_rect = bento_nano_style::Rect {
                    x: retry_row.x + retry_row.width * stealth_value_x_frac,
                    y: label_rect.y,
                    width: retry_row.width * (1.0 - stealth_value_x_frac),
                    height: 16.0,
                };
                let retry_text = smol_str::SmolStr::new(
                    stealth_snapshot
                        .as_ref()
                        .map(|s| s.retry_count)
                        .unwrap_or(0)
                        .to_string(),
                );
                self.draw_text_no_wrap(retry_text.as_str(), value_rect, title_color)?;
            }
        }
        // Row 4 — last-error block (label line + wrapped code), ONLY when set.
        if has_error {
            let err_block = settings_stealth_error_block_rect(
                viewport,
                scroll,
                crash_restart_on,
                safe_start_on,
                has_retry,
            );
            if row_visible(err_block, body) {
                let label_rect = bento_nano_style::Rect {
                    x: err_block.x,
                    y: err_block.y,
                    width: err_block.width,
                    height: 16.0,
                };
                self.draw_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_LAST_ERROR),
                    label_rect,
                    label_color,
                )?;
                let err_rect = bento_nano_style::Rect {
                    x: err_block.x,
                    y: err_block.y + 18.0,
                    width: err_block.width,
                    height: err_block.height - 18.0,
                };
                if let Some(s) = stealth_snapshot.as_ref() {
                    if let Some(err) = s.last_error.as_deref() {
                        self.draw_text(err, err_rect, with_alpha(palette.accent_red, 0.9))?;
                    }
                }
            }
        }
        // Buttons row — [Refresh][Reapply], always shown.
        let stealth_btn_row = settings_stealth_buttons_row_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
            has_retry,
            has_error,
        );
        if row_visible(stealth_btn_row, body) {
            let refresh_btn = settings_stealth_refresh_button_rect(stealth_btn_row);
            self.fill_rounded_rect(refresh_btn, chip_bg, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_REFRESH_BTN),
                refresh_btn,
                title_color,
            )?;
            let reapply_btn = settings_stealth_reapply_button_rect(stealth_btn_row);
            self.fill_rounded_rect(reapply_btn, accent_on, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_REAPPLY_BTN),
                reapply_btn,
                bento_nano_style::Color::WHITE,
            )?;
        }
        // OneDrive warning block — informational text only, ONLY when
        // retry_count > 0 (the backend notes OneDrive typically holds the
        // lock). No button: there is no OneDrive-exclusion probe / guide URL
        // in the nano backend, so per §17 this stays text-only rather than a
        // dead button.
        if has_retry {
            let od_block = settings_stealth_onedrive_block_rect(
                viewport,
                scroll,
                crash_restart_on,
                safe_start_on,
                has_retry,
                has_error,
            );
            if row_visible(od_block, body) {
                let od_bg = with_alpha(
                    bento_nano_style::Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF),
                    0.12,
                );
                self.fill_rounded_rect(od_block, od_bg, chip_radius)?;
                let text_rect = bento_nano_style::Rect {
                    x: od_block.x + 10.0,
                    y: od_block.y + 8.0,
                    width: (od_block.width - 20.0).max(0.0),
                    height: (od_block.height - 16.0).max(0.0),
                };
                self.draw_text(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::STEALTH_ONEDRIVE_WARNING,
                    ),
                    text_rect,
                    with_alpha(title_color, 0.92),
                )?;
            }
        }

        // ── M1f — Updater §8 card (`UpdaterCard.tsx`) ───────────────────
        //
        // Sits after Stealth in the Tauri body order. Reads the live
        // `app.settings_updater_status` snapshot (drained from the
        // UpdateEvent channel by the shell event loop). Status → pill kind +
        // label, version-block / progress-bar / error-line visibility, and
        // action-button visibility all derive from the lib helpers in
        // `business::settings::updater_card` (1:1 with Tauri `statusPillLabel`
        // + the three `<Show when=…>` gates). The conditional middle block's
        // height is captured as `UpdaterHeightKind`, threaded through the same
        // `SettingsBodyFlags` the hit-tester + scroll-clamp use so paint and
        // hit geometry agree.
        use crate::business::settings::updater_card as upd;
        let updater_status = app.settings_updater_status.borrow();
        let updater_flags = SettingsBodyFlags::new(
            crash_restart_on,
            safe_start_on,
            has_retry,
            has_error,
            upd::updater_height_kind(&updater_status),
        );
        let updater_label = settings_updater_label_rect(
            viewport,
            scroll,
            crash_restart_on,
            safe_start_on,
            has_retry,
            has_error,
        );
        if row_visible(updater_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CARD_TITLE),
                updater_label,
                label_color,
            )?;
        }
        // Row 0 — status (label + colored pill), always shown.
        let upd_value_x_frac = 0.5_f32;
        let upd_status_row = settings_updater_status_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_status_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_status_row.x,
                y: upd_status_row.y + (upd_status_row.height - 16.0) * 0.5,
                width: upd_status_row.width * upd_value_x_frac,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_STATUS_LABEL),
                label_rect,
                label_color,
            )?;
            // Pill kind → colour: UpToDate/Ready = green, Busy/Skipped = grey,
            // Active (available/downloading/installing) = blue, Error = red.
            let pill = settings_updater_pill_rect(upd_status_row);
            let pill_radius = bento_nano_style::BorderRadius::all(pill.height * 0.5);
            let pill_bg = match upd::UpdaterPillKind::from_status(&updater_status) {
                upd::UpdaterPillKind::UpToDate | upd::UpdaterPillKind::Ready => {
                    with_alpha(bento_nano_style::Color::from_u8(0x36, 0xC8, 0x6B, 0xFF), 0.90)
                }
                upd::UpdaterPillKind::Active => with_alpha(accent_on, 0.90),
                upd::UpdaterPillKind::Busy | upd::UpdaterPillKind::Skipped => {
                    with_alpha(palette.surface_subtle, 0.85)
                }
                upd::UpdaterPillKind::Error => with_alpha(palette.accent_red, 0.90),
            };
            self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(upd::updater_status_label_id(&updater_status)),
                pill,
                bento_nano_style::Color::WHITE,
            )?;
        }
        // Middle block — version line (Available/Ready/Installing/Skipped),
        // progress bar (Downloading), or error line (Error). Mutually
        // exclusive; StatusOnly paints nothing (zero-height block).
        let upd_middle = settings_updater_middle_block_rect(viewport, scroll, &updater_flags);
        if upd_middle.height > 0.0 && row_visible(upd_middle, body) {
            match updater_flags.updater_kind {
                UpdaterHeightKind::Versioned => {
                    let label_rect = bento_nano_style::Rect {
                        x: upd_middle.x,
                        y: upd_middle.y + (upd_middle.height - 16.0) * 0.5,
                        width: upd_middle.width * upd_value_x_frac,
                        height: 16.0,
                    };
                    self.draw_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::UPDATER_AVAILABLE_VERSION,
                        ),
                        label_rect,
                        label_color,
                    )?;
                    let value_rect = bento_nano_style::Rect {
                        x: upd_middle.x + upd_middle.width * upd_value_x_frac,
                        y: label_rect.y,
                        width: upd_middle.width * (1.0 - upd_value_x_frac),
                        height: 16.0,
                    };
                    if let Some(version) = upd::updater_visible_version(&updater_status) {
                        self.draw_text_no_wrap(version.as_str(), value_rect, title_color)?;
                    }
                }
                UpdaterHeightKind::Downloading => {
                    // Track + filled portion. When the total is unknown the
                    // fraction is None → paint a muted full-width track only
                    // (indeterminate cue), never a panic / divide-by-zero.
                    let track = settings_updater_progress_track_rect(viewport, scroll, &updater_flags);
                    let track_radius =
                        bento_nano_style::BorderRadius::all(track.height * 0.5);
                    self.fill_rounded_rect(
                        track,
                        with_alpha(palette.surface_subtle, 0.85),
                        track_radius,
                    )?;
                    if let Some(frac) = upd::updater_progress_fraction(&updater_status) {
                        let fill = bento_nano_style::Rect {
                            x: track.x,
                            y: track.y,
                            width: (track.width * frac).max(0.0),
                            height: track.height,
                        };
                        self.fill_rounded_rect(fill, accent_on, track_radius)?;
                    }
                }
                UpdaterHeightKind::Error => {
                    if let SettingsUpdaterStatus::Error(message) = &*updater_status {
                        self.draw_text(
                            message.as_str(),
                            upd_middle,
                            with_alpha(palette.accent_red, 0.9),
                        )?;
                    }
                }
                UpdaterHeightKind::StatusOnly => {}
            }
        }
        // Action buttons row — 检查更新 (always, col 0), then state-gated
        // 下载 / 安装并重启 (col 1) + 跳过此版本 (col 2). The column indices match
        // the hit-tester so paint and hit agree.
        let upd_btn_row = settings_updater_buttons_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_btn_row, body) {
            // Col 0 — 检查更新 (always).
            let check_btn = settings_updater_button_rect(upd_btn_row, 0);
            self.fill_rounded_rect(check_btn, chip_bg, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CHECK_NOW),
                check_btn,
                title_color,
            )?;
            // Col 1 — 下载 (Available) or 安装并重启 (Ready), accent-filled.
            if upd::updater_show_download(&updater_status) {
                let dl_btn = settings_updater_button_rect(upd_btn_row, 1);
                self.fill_rounded_rect(dl_btn, accent_on, btn_radius)?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_DOWNLOAD),
                    dl_btn,
                    bento_nano_style::Color::WHITE,
                )?;
            } else if upd::updater_show_install(&updater_status) {
                let install_btn = settings_updater_button_rect(upd_btn_row, 1);
                self.fill_rounded_rect(install_btn, accent_on, btn_radius)?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::UPDATER_INSTALL_RESTART,
                    ),
                    install_btn,
                    bento_nano_style::Color::WHITE,
                )?;
            }
            // Col 2 — 跳过此版本 (Available/Ready), neutral chip.
            if upd::updater_show_skip(&updater_status) {
                let skip_btn = settings_updater_button_rect(upd_btn_row, 2);
                self.fill_rounded_rect(skip_btn, chip_bg, btn_radius)?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_SKIP_VERSION),
                    skip_btn,
                    title_color,
                )?;
            }
        }
        // Prefs row — 检查频率 cycling chip (Daily/Weekly/Manual).
        let upd_freq_row = settings_updater_frequency_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_freq_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_freq_row.x,
                y: upd_freq_row.y + (upd_freq_row.height - 16.0) * 0.5,
                width: upd_freq_row.width * upd_value_x_frac,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQUENCY),
                label_rect,
                label_color,
            )?;
            let chip = settings_updater_frequency_chip_rect(upd_freq_row);
            self.fill_rounded_rect(chip, chip_bg, chip_radius)?;
            let freq_id = match app.update_check_frequency.get() {
                bento_nano_backend::updater::UpdateCheckFrequency::Daily => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_DAILY
                }
                bento_nano_backend::updater::UpdateCheckFrequency::Weekly => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_WEEKLY
                }
                bento_nano_backend::updater::UpdateCheckFrequency::Manual => {
                    bento_nano_style::i18n_zh_cn::ids::UPDATER_FREQ_MANUAL
                }
            };
            self.draw_text_no_wrap(bento_nano_style::t(freq_id), chip, title_color)?;
        }
        // Prefs row — 后台静默下载 toggle.
        let upd_auto_row = settings_updater_auto_download_row_rect(viewport, scroll, &updater_flags);
        if row_visible(upd_auto_row, body) {
            let label_rect = bento_nano_style::Rect {
                x: upd_auto_row.x,
                y: upd_auto_row.y + (upd_auto_row.height - 16.0) * 0.5,
                width: upd_auto_row.width * 0.7,
                height: 16.0,
            };
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_AUTO_DOWNLOAD),
                label_rect,
                label_color,
            )?;
            let auto_on = app.update_auto_download.get();
            let hit = settings_updater_auto_download_hit_rect(upd_auto_row);
            let switch = toggle_switch_in_rect(hit);
            self.fill_rounded_rect(
                switch.track,
                if auto_on { accent_on } else { track_off },
                BorderRadius::all(switch.track_radius()),
            )?;
            self.fill_rounded_rect(
                switch.knob(auto_on),
                knob_color,
                BorderRadius::all(switch.knob_radius()),
            )?;
        }
        drop(updater_status);

        // ── M1g — Backup §9 card (`BackupCard.tsx`) ─────────────────────
        //
        // Sits after Updater in the Tauri body order. Reads the live
        // `app.settings_backup_entries` snapshot (populated on Settings open +
        // after every create/restore by the shell). The list is
        // variable-length, capped at SETTINGS_BACKUP_ROW_VISIBLE_MAX; the
        // capped count threads through the same `SettingsBodyFlags` the
        // hit-tester + scroll-clamp use (via `with_backup_rows`) so paint and
        // hit geometry agree. Size + empty-state + the capped count come from
        // the lib helpers in `business::settings::backup_card`.
        use crate::business::settings::backup_card as bkp;
        // Snapshot the entries + status text out of the RefCells BEFORE the
        // fallible paint calls so no borrow spans them (mirrors the Stealth
        // snapshot pattern above).
        let backup_entries = app.settings_backup_entries.borrow().clone();
        let backup_status_snapshot = app.settings_backup_status.borrow().clone();
        let backup_visible = bkp::backup_visible_row_count(&backup_entries);
        let backup_flags = updater_flags.with_backup_rows(backup_visible);
        let backup_label = settings_backup_label_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_CARD_TITLE),
                backup_label,
                label_color,
            )?;
        }
        // Description line — always shown.
        let backup_desc = settings_backup_description_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_desc, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_CARD_DESCRIPTION),
                backup_desc,
                label_color,
            )?;
        }
        // Actions row — [立即备份 (accent)] [刷新 (neutral)].
        let backup_actions = settings_backup_actions_row_rect(viewport, scroll, &backup_flags);
        if row_visible(backup_actions, body) {
            let create_btn = settings_backup_create_button_rect(backup_actions);
            self.fill_rounded_rect(create_btn, accent_on, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_CREATE_NOW),
                create_btn,
                bento_nano_style::Color::WHITE,
            )?;
            let refresh_btn = settings_backup_refresh_button_rect(backup_actions);
            self.fill_rounded_rect(refresh_btn, chip_bg, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_REFRESH),
                refresh_btn,
                title_color,
            )?;
        }
        // Info/error line — only when a status is set. Success → green, error
        // → red (mirrors the widget-tree card's status colours).
        if let Some(status) = backup_status_snapshot.as_ref() {
            let backup_status_row = settings_backup_status_rect(viewport, scroll, &backup_flags);
            if row_visible(backup_status_row, body) {
                let is_error = matches!(status, crate::state::SettingsBackupStatus::Error(_));
                let status_color = if is_error {
                    with_alpha(palette.accent_red, 0.9)
                } else {
                    with_alpha(bento_nano_style::Color::from_u8(0x36, 0xC8, 0x6B, 0xFF), 0.9)
                };
                self.draw_text(bkp::backup_status_text(status), backup_status_row, status_color)?;
            }
        }
        // Backup list — N entry rows (file·size + 恢复) or one backupEmpty
        // placeholder. Both branches anchor off the reserved status slot so the
        // list lines up whether or not a status line painted.
        if bkp::backup_list_is_empty(&backup_entries) {
            let empty_row = settings_backup_entry_row_rect(viewport, scroll, &backup_flags, 0);
            if row_visible(empty_row, body) {
                self.draw_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_EMPTY),
                    empty_row,
                    label_color,
                )?;
            }
        } else {
            for (entry_index, entry) in backup_entries
                .iter()
                .take(SETTINGS_BACKUP_ROW_VISIBLE_MAX)
                .enumerate()
            {
                let entry_row =
                    settings_backup_entry_row_rect(viewport, scroll, &backup_flags, entry_index);
                if !row_visible(entry_row, body) {
                    continue;
                }
                // Left — "id · size" (the backend chose the stable id over a
                // parsed timestamp; reuse the same label the widget card uses).
                // Format the size once per visible row (§10 — no redundant
                // per-frame formatting beyond the visible rows).
                let restore_btn = settings_backup_restore_button_rect(entry_row);
                let info_rect = bento_nano_style::Rect {
                    x: entry_row.x,
                    y: entry_row.y + (entry_row.height - 16.0) * 0.5,
                    width: (restore_btn.x - entry_row.x - 8.0).max(0.0),
                    height: 16.0,
                };
                self.draw_text_no_wrap(
                    bkp::format_entry_label(entry).as_str(),
                    info_rect,
                    title_color,
                )?;
                // Right — 恢复 button (neutral chip).
                self.fill_rounded_rect(restore_btn, chip_bg, btn_radius)?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_RESTORE),
                    restore_btn,
                    title_color,
                )?;
            }
        }

        // ── M1h — Plugins §11 section (`SettingsPanel.tsx:709-781`) ──────
        //
        // Sits LAST in the (currently shipped) Tauri body order
        // (…→Backup→**Plugins**→footer); Encryption §10 is deferred so Plugins
        // anchors directly after the Backup card. Reads the live
        // `app.settings_plugin_entries` snapshot (populated on Settings open +
        // after every install/toggle/uninstall by the shell). The list is
        // variable-length, capped at SETTINGS_PLUGINS_ROW_VISIBLE_MAX; the
        // capped count threads through the same `SettingsBodyFlags` the
        // hit-tester + scroll-clamp use (via `with_plugin_rows`) so paint and
        // hit geometry agree. PURE view-model helpers (badge id, visible cap,
        // empty predicate, header text) come from
        // `business::settings::plugins_section`. Dark dialog tokens only — the
        // old modal's light `active_theme_palette()` was dropped.
        use crate::business::settings::plugins_section as plg;
        use crate::settings_panel::{
            settings_plugin_author_rect, settings_plugin_badge_rect, settings_plugin_card_rect,
            settings_plugin_desc_rect, settings_plugin_empty_row_rect, settings_plugin_name_rect,
            settings_plugin_toggle_hit_rect, settings_plugin_uninstall_button_rect,
            settings_plugins_install_button_rect, settings_plugins_label_rect,
            SETTINGS_PLUGINS_ROW_VISIBLE_MAX,
        };
        // Snapshot the entries out of the RefCell BEFORE the fallible paint
        // calls so no borrow spans them (mirrors the Backup/Stealth pattern).
        let plugin_entries = app.settings_plugin_entries.borrow().clone();
        let plugin_visible = plg::plugin_visible_row_count(&plugin_entries);
        let plugin_flags = backup_flags.with_plugin_rows(plugin_visible);
        // Group title — 插件 / Plugins (reuses SETTINGS_PLUGINS id 36).
        let plugin_label = settings_plugins_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(plugin_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_PLUGINS),
                plugin_label,
                label_color,
            )?;
        }
        // Full-width 安装插件... button (neutral chip) → InstallPlugin.
        let plugin_install = settings_plugins_install_button_rect(viewport, scroll, &plugin_flags);
        if row_visible(plugin_install, body) {
            self.fill_rounded_rect(plugin_install, chip_bg, btn_radius)?;
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_INSTALL),
                plugin_install,
                title_color,
            )?;
        }
        // plugin-list — N plugin cards or one pluginEmpty placeholder.
        if plg::plugin_list_is_empty(&plugin_entries) {
            let empty_row = settings_plugin_empty_row_rect(viewport, scroll, &plugin_flags);
            if row_visible(empty_row, body) {
                self.draw_text(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_EMPTY),
                    empty_row,
                    label_color,
                )?;
            }
        } else {
            for (card_index, plugin) in plugin_entries
                .iter()
                .take(SETTINGS_PLUGINS_ROW_VISIBLE_MAX)
                .enumerate()
            {
                let card = settings_plugin_card_rect(viewport, scroll, &plugin_flags, card_index);
                if !row_visible(card, body) {
                    continue;
                }
                // Card surface — raised chip behind the whole card.
                self.fill_rounded_rect(card, chip_bg, chip_radius)?;
                // Header — name · v{version} (left), type badge + enable toggle
                // (right). The header text is formatted once per visible card.
                let name_rect = settings_plugin_name_rect(card);
                self.draw_text_no_wrap(
                    plg::format_plugin_header(plugin).as_str(),
                    name_rect,
                    title_color,
                )?;
                // Type badge — accent-tinted chip (theme=purple, widget=blue,
                // organizer=green; `SettingsPanel.css:612-625`).
                let badge_rect = settings_plugin_badge_rect(card);
                let badge_accent = match plugin.plugin_type.as_str() {
                    "widget" => palette.accent_blue,
                    "organizer" => palette.accent_green,
                    _ => palette.accent_purple,
                };
                self.fill_rounded_rect(
                    badge_rect,
                    with_alpha(badge_accent, 0.20),
                    bento_nano_style::BorderRadius::all(badge_rect.height * 0.5),
                )?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(plg::plugin_type_label_id(plugin.plugin_type.as_str())),
                    badge_rect,
                    with_alpha(badge_accent, 1.0),
                )?;
                // Enable toggle — accent when on, neutral track when off →
                // TogglePlugin(card_index).
                let toggle_rect = settings_plugin_toggle_hit_rect(card);
                let toggle_radius = bento_nano_style::BorderRadius::all(toggle_rect.height * 0.5);
                self.fill_rounded_rect(
                    toggle_rect,
                    if plugin.enabled {
                        accent_on
                    } else {
                        track_off
                    },
                    toggle_radius,
                )?;
                self.draw_text_no_wrap(
                    if plugin.enabled {
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_ON)
                    } else {
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_OFF)
                    },
                    toggle_rect,
                    bento_nano_style::Color::WHITE,
                )?;
                // Author line (muted).
                let author_rect = settings_plugin_author_rect(card);
                self.draw_text_no_wrap(
                    plugin.author.as_str(),
                    author_rect,
                    with_alpha(palette.text_muted, 0.95),
                )?;
                // Description line (muted).
                let desc_rect = settings_plugin_desc_rect(card);
                self.draw_text_no_wrap(
                    plugin.description.as_str(),
                    desc_rect,
                    with_alpha(palette.text_muted, 0.95),
                )?;
                // Actions — 卸载 / Uninstall (danger chip) → UninstallPlugin(idx).
                let uninstall_btn = settings_plugin_uninstall_button_rect(card);
                self.fill_rounded_rect(
                    uninstall_btn,
                    with_alpha(palette.accent_red, 0.85),
                    btn_radius,
                )?;
                self.draw_text_no_wrap(
                    bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_UNINSTALL),
                    uninstall_btn,
                    bento_nano_style::Color::WHITE,
                )?;
            }
        }

        // ── M6-UI — §3 Appearance inline theme grid (`SettingsPanel.tsx:396-536`) ──
        //
        // Flows LAST in the nano body order (…→Plugins→**Appearance**). The
        // grid geometry (group headings + 17 ThemeCards + accent swatch row) is
        // owned by `theme_picker::appearance_layout`; the section anchor +
        // content width come from `settings_panel`. Selecting a card re-skins
        // the app live (the active card draws a 2-DIP accent-blue border + a
        // 10%-blue fill tint, compared against `app.active_theme_id`). The
        // accent swatch row is the editable accent picker (Control B MVP).
        //
        // Developer Options (custom-theme textarea + Import/Export) is DEFERRED
        // (no nano keyboard/text-input infra + no JSON theme parser) — see the
        // M6-UI carve-out note; no dead toggle is painted.
        use crate::settings_panel::{
            settings_appearance_grid_origin, settings_appearance_inner_width,
            settings_appearance_label_rect, settings_appearance_picker_label_rect,
        };
        use crate::theme_picker::{
            self as tp, AppearanceLayout, BUILTIN_THEMES, SWATCH_BLOCK_RADIUS, SWATCH_INNER_GAP,
            THEME_CARD_BORDER, THEME_CARD_RADIUS, THEME_GROUP_ORDER,
        };
        // Live theme id (the active card highlight) — borrowed once.
        let active_theme_id = app.active_theme_id.borrow().clone();
        // Live accent (the ringed accent swatch) — the in-flight draft wins,
        // else the persisted theme-base accent. Owned snapshot so no RefCell
        // borrow spans the fallible paint calls below.
        let active_accent: Option<smol_str::SmolStr> = app
            .settings_draft_accent_color
            .borrow()
            .clone()
            .or_else(|| app.theme_base_accent.borrow().clone());
        // Group title — 外观 / Appearance.
        let appearance_label = settings_appearance_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(appearance_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_APPEARANCE),
                appearance_label,
                label_color,
            )?;
        }
        // "选择主题 / Choose Theme" picker label.
        let picker_label = settings_appearance_picker_label_rect(viewport, scroll, &plugin_flags);
        if row_visible(picker_label, body) {
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::THEME_PICKER_LABEL),
                picker_label,
                label_color,
            )?;
        }
        // Grid layout — body-width-driven, Copy, allocation-free.
        let appearance_origin = settings_appearance_grid_origin(viewport, scroll, &plugin_flags);
        let appearance_inner_w = settings_appearance_inner_width(viewport);
        let appearance: AppearanceLayout = tp::appearance_layout(appearance_origin, appearance_inner_w);
        // surface_subtle = rgba(white, 0.04) card bg (live theme). Active card
        // overrides to accent-blue@0.10 + a 2-DIP accent-blue rounded border.
        let card_bg = palette.surface_subtle;
        let active_card_bg = with_alpha(palette.accent_blue, 0.10);
        let card_radius = bento_nano_style::BorderRadius::all(THEME_CARD_RADIUS);
        let swatch_radius = bento_nano_style::BorderRadius::all(SWATCH_BLOCK_RADIUS);
        // Group headings — Tauri `.theme-group__title`: UPPERCASE,
        // letter-spacing 1px, font-size 10px, weight 600, color text-muted.
        // `draw_text_tracked` upper-cases (no-op for CJK) + applies the 1-DIP
        // per-glyph tracking via DWrite SetCharacterSpacing (both locales).
        for (group_pos, group) in THEME_GROUP_ORDER.iter().enumerate() {
            let heading = appearance.group_headings[group_pos];
            if row_visible(heading, body) {
                self.draw_text_tracked(
                    bento_nano_style::t(group.heading_id()),
                    heading,
                    palette.text_muted,
                    10.0,
                    600,
                    1.0,
                )?;
            }
        }
        // 17 ThemeCards (walk the preset table; rects indexed by preset id).
        for preset in BUILTIN_THEMES.iter() {
            let i = preset.id as usize;
            let card = appearance.cards[i];
            if !row_visible(card, body) {
                continue;
            }
            let is_active = preset.theme_id == active_theme_id.as_str();
            // Card surface.
            self.fill_rounded_rect(card, if is_active { active_card_bg } else { card_bg }, card_radius)?;
            // Active card border — 2-DIP accent-blue. Tauri's CSS `border` is a
            // fully-inset border-box; D2D strokes centred on the geometric edge,
            // so the rect is inset by half the stroke width (1 DIP) on all sides
            // and the radius shrinks to stay concentric — no bleed past the card.
            if is_active {
                let inset = THEME_CARD_BORDER * 0.5;
                let border_rect = bento_nano_style::Rect {
                    x: card.x + inset,
                    y: card.y + inset,
                    width: (card.width - THEME_CARD_BORDER).max(0.0),
                    height: (card.height - THEME_CARD_BORDER).max(0.0),
                };
                let border_radius =
                    bento_nano_style::BorderRadius::all((THEME_CARD_RADIUS - inset).max(0.0));
                self.stroke_rounded_rect(
                    border_rect,
                    palette.accent_blue,
                    border_radius,
                    THEME_CARD_BORDER,
                )?;
            }
            // 40×40 swatch block — 4 quadrant fills (3-DIP gutter == gap:3px).
            let block = appearance.swatch_blocks[i];
            // Drop shadow behind the block — Tauri `.theme-card__swatches`
            // `box-shadow: 0 1px 3px rgba(0,0,0,0.15)`. Simulated (as elsewhere
            // in this renderer) by a translucent rounded fill offset +1 DIP in Y
            // and spread the 3-DIP blur on every side, painted before the block.
            const SWATCH_SHADOW_OFFSET_Y: f32 = 1.0;
            const SWATCH_SHADOW_BLUR: f32 = 3.0;
            let block_shadow = bento_nano_style::Rect {
                x: block.x - SWATCH_SHADOW_BLUR,
                y: block.y + SWATCH_SHADOW_OFFSET_Y - SWATCH_SHADOW_BLUR,
                width: block.width + SWATCH_SHADOW_BLUR * 2.0,
                height: block.height + SWATCH_SHADOW_BLUR * 2.0,
            };
            self.fill_rounded_rect(
                block_shadow,
                with_alpha(Color::BLACK, 0.15),
                bento_nano_style::BorderRadius::all(SWATCH_BLOCK_RADIUS + SWATCH_SHADOW_BLUR),
            )?;
            // Block pad behind the quadrants (rounded clip silhouette).
            self.fill_rounded_rect(block, palette.surface_subtle, swatch_radius)?;
            // Quadrants — Tauri `.theme-card__swatches { border-radius:8;
            // overflow:hidden }` masks SHARP-cornered quadrants behind an 8-DIP
            // rounded square. No rounded-clip primitive exists (PushAxisAlignedClip
            // is rectangular), so each corner quadrant rounds ONLY its single
            // OUTER corner to 8 (TL→top-left, TR→top-right, BL→bottom-left,
            // BR→bottom-right) and stays square at the inner centre cross — the
            // visible-correct per-corner approximation via `fill_partial_rounded_rect`.
            const QUADRANT_OUTER_CORNER: [[bool; 4]; 4] = [
                [true, false, false, false],  // 0 = TL
                [false, true, false, false],  // 1 = TR
                [false, false, false, true],  // 2 = BL
                [false, false, true, false],  // 3 = BR
            ];
            let quads = tp::thumbnail_swatch_quadrants(block, SWATCH_INNER_GAP);
            let mut q = 0usize;
            while q < 4 {
                self.fill_partial_rounded_rect(
                    quads[q],
                    preset.swatch_colors[q],
                    SWATCH_BLOCK_RADIUS,
                    QUADRANT_OUTER_CORNER[q],
                )?;
                q += 1;
            }
            // Name label below the swatch — Tauri `.theme-card__label`:
            // text-align:center, 10px, color text-secondary, single line.
            let label_rect = bento_nano_style::Rect {
                x: card.x,
                y: block.bottom() + crate::theme_picker::THEME_CARD_SWATCH_LABEL_GAP,
                width: card.width,
                height: crate::theme_picker::CARD_LABEL_HEIGHT,
            };
            self.draw_text_centered(
                bento_nano_style::t(preset.name_id),
                label_rect,
                palette.text_secondary,
                10.0,
                400,
            )?;
        }
        // Accent row (Control B) — label + 12-swatch VIBRANT strip + value ring.
        if row_visible(appearance.accent_row, body) {
            let accent_label_rect = bento_nano_style::Rect {
                x: appearance.accent_row.x,
                y: appearance.accent_row.y,
                width: appearance.accent_row.width * 0.5,
                height: appearance.accent_row.height,
            };
            self.draw_text_no_wrap(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_ACCENT_COLOR),
                accent_label_rect,
                label_color,
            )?;
        }
        for s in 0..crate::theme_picker::ACCENT_SWATCH_COUNT {
            let dot = appearance.accent_swatches[s];
            if !row_visible(dot, body) {
                continue;
            }
            let dot_radius = bento_nano_style::BorderRadius::all(dot.height * 0.5);
            self.fill_rounded_rect(dot, crate::theme_picker::ACCENT_SWATCHES[s], dot_radius)?;
            // Current-value ring on the active accent swatch.
            let is_active_accent = active_accent
                .as_deref()
                .map(|hex| crate::theme_picker::accent_swatch_hex(s) == Some(hex))
                .unwrap_or(false);
            if is_active_accent {
                let ring = bento_nano_style::Rect {
                    x: dot.x - 2.0,
                    y: dot.y - 2.0,
                    width: dot.width + 4.0,
                    height: dot.height + 4.0,
                };
                self.stroke_rounded_rect(
                    ring,
                    palette.text_primary,
                    bento_nano_style::BorderRadius::all(ring.height * 0.5),
                    2.0,
                )?;
            }
        }

            Ok(())
        })();
        // Balance the body clip BEFORE propagating any body-paint error so the
        // device context is never left with a dangling PushAxisAlignedClip.
        self.pop_clip()?;
        body_paint?;

        // 5) Footer (sticky, 56 DIP) — [取消] [保存(accent)]. Painted AFTER the
        // body clip is popped so the sticky footer is never masked by it.
        let footer = settings_footer_rect(viewport);
        self.fill_rounded_rect(footer, footer_bg, footer_radius)?;
        let footer_hairline = bento_nano_style::Rect {
            x: footer.x,
            y: footer.y,
            width: footer.width,
            height: 1.0,
        };
        self.fill_rounded_rect(footer_hairline, divider_color, BorderRadius::ZERO)?;
        let cancel_btn = settings_cancel_button_rect(viewport);
        self.fill_rounded_rect(cancel_btn, chip_bg, btn_radius)?;
        self.draw_text_no_wrap(
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CANCEL),
            cancel_btn,
            title_color,
        )?;
        // M1a 2026-05-29 — Save dims to ~0.4 alpha when no toggle has been
        // touched since the panel opened, mirroring Tauri `disabled={!dirty()}`
        // at `SettingsPanel.tsx:799`. The hit-tester treats the dimmed button
        // as a no-op (`SaveSettings` dispatch arm short-circuits when
        // `!settings_dirty`); Cancel stays always-active.
        let save_btn = settings_save_button_rect(viewport);
        let dirty = app.settings_dirty.get();
        let save_alpha: f32 = if dirty { 1.0 } else { 0.4 };
        self.fill_rounded_rect(save_btn, with_alpha(accent_on, save_alpha), btn_radius)?;
        self.draw_text_no_wrap(
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_SAVE),
            save_btn,
            with_alpha(bento_nano_style::Color::WHITE, save_alpha),
        )?;

        // K1 modal-opener paint paths — orphan-alive per Ruling B. They never
        // fire from M1 hit-test (no SettingsHit→Open* arms) but compile and
        // can still surface via keyboard shortcuts.
        if app.settings_keybindings_open.get() {
            self.draw_keybindings_modal(app)?;
        }
        // M1h (2026-05-29) — the plugins MODAL gate (`if app.settings_plugins_open
        // { self.draw_plugins_modal(app) }`) was removed: the Plugins surface is
        // now an always-inline §11 section painted inside the scrollable body
        // (see the M1h block in the body-paint closure above). `draw_plugins_modal`
        // + `settings_plugins_open` were deleted.
        // M6-UI (2026-05-29) — the Wave J1b swatch-popup paint
        // (`if app.theme_picker_open { paint_into(ThemePickerAdapter, …) }`)
        // was removed: §3 Appearance is now an always-inline grid painted by
        // the M6-UI block inside the scrollable body-paint closure above
        // (group headings + 17 ThemeCards + accent swatch row), re-skinning
        // live off `app.active_theme_tauri()`.

        Ok(())
    }

    // M1h (2026-05-29) — `draw_plugins_modal` was deleted. The plugins surface
    // moved from a gated, light-`active_theme_palette()` in-panel MODAL to an
    // always-inline §11 section of the dark scrollable Settings body, painted by
    // the M1h block inside `draw_settings_panel`'s body-paint closure (dark
    // dialog tokens, full-width Install button, plugin-card list with type
    // badge + toggle + author + description + Uninstall). Reachability is
    // unchanged: Install → `InstallPlugin` (file picker), per-card toggle →
    // `TogglePlugin(idx)`, per-card uninstall → `UninstallPlugin(idx)`.

    /// Draw the selected-stack keybindings recorder/reset modal. This is the
    /// native D2D replacement for the Tauri KeybindingsSection portal: rows
    /// come from the shared settings action catalog, current chords are read
    /// from the real config vault, and capture/reset results are rendered
    /// visibly per action.
    fn draw_keybindings_modal(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::settings::keybindings_section;
        use crate::settings_panel::{
            settings_keybinding_record_rect, settings_keybinding_reset_rect,
            settings_keybinding_row_rect, settings_keybindings_close_rect,
            settings_keybindings_modal_rect, settings_panel_shadow_rect,
        };
        let palette = app.active_theme_palette();
        let radius_tokens = app.active_theme_radius();
        let spacing_tokens = app.active_theme_spacing();
        let shadow_tokens = app.active_theme_shadow();
        let modal_scrim = with_alpha(palette.scrim, 0.45);
        let modal_bg = with_alpha(palette.surface, 0.98);
        let title_color = with_alpha(palette.text, 0.96);
        let label_color = with_alpha(palette.text, 0.94);
        let muted_text = with_alpha(palette.text_muted, 0.95);
        let btn_bg = with_alpha(palette.accent, 0.80);
        let btn_disabled_bg = with_alpha(palette.surface_alt, 0.78);
        let chip_bg = with_alpha(palette.surface_alt, 0.96);
        let success_text = with_alpha(palette.success, 0.95);
        let error_text = with_alpha(palette.danger, 0.95);
        let modal_radius = radius_tokens.xl;
        let control_radius = radius_tokens.md;
        let panel_shadow = shadow_tokens.lg;
        let title_pad_x = spacing_tokens.xl;
        let title_pad_y = spacing_tokens.lg;
        let control_pad_x = spacing_tokens.md;
        let control_pad_y = spacing_tokens.xs + 1.0;
        let close_pad_x = (spacing_tokens.lg - spacing_tokens.xs).max(0.0);
        let control_text_rect = |rect: Rect| Rect {
            x: rect.x + control_pad_x,
            y: rect.y + control_pad_y,
            width: (rect.width - control_pad_x * 2.0).max(0.0),
            height: (rect.height - control_pad_y * 2.0).max(0.0),
        };

        let viewport = app.viewport;
        let scrim_rect = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };
        self.fill_rounded_rect(scrim_rect, modal_scrim, BorderRadius::ZERO)?;

        let modal = settings_keybindings_modal_rect(viewport);
        let modal_shadow_rect = settings_panel_shadow_rect(modal, panel_shadow);
        self.fill_rounded_rect(modal_shadow_rect, panel_shadow.color, modal_radius)?;
        self.fill_rounded_rect(modal, modal_bg, modal_radius)?;

        let title_rect = bento_nano_style::Rect {
            x: modal.x + title_pad_x,
            y: modal.y + title_pad_y,
            width: modal.width - title_pad_x * 2.0,
            height: 24.0,
        };
        // M6c — keybindings modal title (`h2` panel header).
        self.draw_text_chromatic_title(
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_TITLE),
            title_rect,
            title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = settings_keybindings_close_rect(viewport);
        self.fill_rounded_rect(close_rect, btn_bg, control_radius)?;
        self.draw_text(
            "×",
            bento_nano_style::Rect {
                x: close_rect.x + close_pad_x,
                y: close_rect.y + spacing_tokens.xs,
                width: (close_rect.width - close_pad_x * 2.0).max(0.0),
                height: (close_rect.height - spacing_tokens.sm).max(0.0),
            },
            title_color,
        )?;

        let recording = app.settings_keybinding_recording.borrow().clone();
        let feedback = app.settings_keybinding_feedback.borrow().clone();
        for (row_index, row) in keybindings_section::keybinding_rows().iter().enumerate() {
            let row_rect = settings_keybinding_row_rect(viewport, row_index);
            let record_rect = settings_keybinding_record_rect(viewport, row_index);
            let reset_rect = settings_keybinding_reset_rect(viewport, row_index);
            let recording_this = recording.as_deref() == Some(row.action);
            let recording_other = recording.is_some() && !recording_this;

            let label_rect = bento_nano_style::Rect {
                x: row_rect.x,
                y: row_rect.y + spacing_tokens.xs,
                width: 138.0,
                height: 16.0,
            };
            self.draw_text(row.label, label_rect, label_color)?;

            let chip_rect = bento_nano_style::Rect {
                x: row_rect.x + 146.0,
                y: row_rect.y + spacing_tokens.xs,
                width: 116.0,
                height: 22.0,
            };
            self.fill_rounded_rect(chip_rect, chip_bg, control_radius)?;
            let chord = if recording_this {
                smol_str::SmolStr::new(bento_nano_style::t(
                    bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RECORDING,
                ))
            } else {
                keybindings_section::current_chord_for_action(row.action).unwrap_or_else(|| {
                    smol_str::SmolStr::new(bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_UNSUPPORTED,
                    ))
                })
            };
            self.draw_text(
                chord.as_str(),
                control_text_rect(chip_rect),
                if recording_this {
                    success_text
                } else {
                    muted_text
                },
            )?;

            self.fill_rounded_rect(
                record_rect,
                if recording_other {
                    btn_disabled_bg
                } else {
                    btn_bg
                },
                control_radius,
            )?;
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RECORD),
                control_text_rect(record_rect),
                if recording_other {
                    muted_text
                } else {
                    title_color
                },
            )?;
            self.fill_rounded_rect(reset_rect, btn_bg, control_radius)?;
            self.draw_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::KEYBINDINGS_RESET),
                control_text_rect(reset_rect),
                title_color,
            )?;

            if let Some(active_feedback) =
                feedback.as_ref().filter(|msg| msg.action() == row.action)
            {
                let feedback_rect = bento_nano_style::Rect {
                    x: row_rect.x,
                    y: row_rect.y + 18.0,
                    width: row_rect.width - 132.0,
                    height: 10.0,
                };
                self.draw_text(
                    active_feedback.message(),
                    feedback_rect,
                    if active_feedback.is_error() {
                        error_text
                    } else {
                        success_text
                    },
                )?;
            }
        }

        Ok(())
    }

    /// Draw the selected-stack About modal. This is runtime-rendered, not
    /// just `business::about::build()` reachability, so the tray About item
    /// produces a visible desktop-software surface in the nano executable.
    fn draw_about_panel(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::about;
        let palette = app.active_theme_palette();
        let radius_tokens = app.active_theme_radius();
        let spacing_tokens = app.active_theme_spacing();
        let shadow_tokens = app.active_theme_shadow();
        let scrim_color = with_alpha(palette.scrim, 0.34);
        let panel_bg = with_alpha(palette.surface, 0.96);
        let title_color = with_alpha(palette.text, 0.96);
        let body_color = with_alpha(palette.text, 0.90);
        let muted_color = with_alpha(palette.text_muted, 0.92);
        let btn_bg = with_alpha(palette.accent, 0.90);
        let btn_text = with_alpha(palette.text, 0.96);
        let panel_radius = radius_tokens.xl;
        let button_radius = radius_tokens.md;
        let panel_shadow = shadow_tokens.lg;

        let viewport = app.viewport;
        let scrim = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        };
        self.fill_rounded_rect(scrim, scrim_color, BorderRadius::ZERO)?;

        let panel = about::panel_rect(viewport);
        let shadow_spread = panel_shadow.blur.max(0.0);
        let shadow_rect = bento_nano_style::Rect {
            x: panel.x + panel_shadow.offset_x - shadow_spread,
            y: panel.y + panel_shadow.offset_y - shadow_spread,
            width: panel.width + shadow_spread * 2.0,
            height: panel.height + shadow_spread * 2.0,
        };
        self.fill_rounded_rect(shadow_rect, panel_shadow.color, panel_radius)?;
        self.fill_rounded_rect(panel, panel_bg, panel_radius)?;

        let title_rect = bento_nano_style::Rect {
            x: panel.x + about::CONTENT_PADDING,
            y: panel.y + spacing_tokens.xxl,
            width: panel.width - about::CONTENT_PADDING * 2.0,
            height: 34.0,
        };
        // M6c — About app-name is the `h1` heading.
        self.draw_text_chromatic_title(
            "BentoDesk",
            title_rect,
            title_color,
            app.active_theme_effect_tauri(),
        )?;

        let version_rect = bento_nano_style::Rect {
            x: title_rect.x,
            y: title_rect.y + 42.0,
            width: title_rect.width,
            height: 24.0,
        };
        self.draw_text(about::VERSION, version_rect, body_color)?;

        let build_rect = bento_nano_style::Rect {
            x: title_rect.x,
            y: version_rect.y + 28.0,
            width: title_rect.width,
            height: 24.0,
        };
        self.draw_text(about::BUILD_HASH, build_rect, muted_color)?;

        let copy_rect = bento_nano_style::Rect {
            x: title_rect.x,
            y: build_rect.y + 32.0,
            width: title_rect.width,
            height: 44.0,
        };
        self.draw_text("Selected-stack desktop refactor", copy_rect, body_color)?;

        let close = about::close_button_rect(viewport);
        self.fill_rounded_rect(close, btn_bg, button_radius)?;
        let close_text = bento_nano_style::Rect {
            x: close.x + spacing_tokens.xl,
            y: close.y + spacing_tokens.sm + spacing_tokens.xs,
            width: (close.width - spacing_tokens.xl * 2.0).max(0.0),
            height: (close.height - spacing_tokens.lg).max(0.0),
        };
        self.draw_text("Close", close_text, btn_text)?;
        Ok(())
    }

    fn draw_tooltip_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let Some(session) = app.active_tooltip.borrow().clone() else {
            return Ok(());
        };
        // Wave E: Tauri SSoT tokens for the tooltip pill.
        use bento_nano_style::tokens as style_tokens;
        let descriptor = tooltip::Tooltip::from_tauri_tokens(
            session.text,
            app.active_theme_tauri(),
            // tooltip radius is global chrome (same for every theme, design §1.2)
            // — the per-theme `RadiusTauri` carries the global tooltip/minibar.
            app.active_theme_radius_tauri(),
            style_tokens::SPACING,
        );
        let pill = tooltip::tooltip_pill_rect(app.viewport);
        self.fill_rounded_rect(pill, descriptor.background, descriptor.border_radius)?;
        let text_rect = tooltip::tooltip_text_rect(app.viewport, &descriptor);
        self.draw_text(descriptor.text.as_str(), text_rect, descriptor.text_color)?;
        Ok(())
    }

    fn draw_minibar_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let Some((zone_id, bar)) = app.active_minibar() else {
            return Ok(());
        };
        // Wave D: paint the MiniBar from Wave B Tauri SSoT tokens (gradient
        // top stop + 14 px radius — Wave A flagged gap).
        use bento_nano_style::tokens as style_tokens;
        let bar = bar.with_tauri_tokens(
            app.active_theme_tauri(),
            // minibar radius is global chrome (same for every theme, design §1.2).
            app.active_theme_radius_tauri(),
            style_tokens::SPACING,
        );
        let viewport = app.viewport;
        let panel = minibar::minibar_panel_rect(viewport);
        self.fill_rounded_rect(panel, bar.background, bar.border_radius)?;

        let icon_rect = minibar::minibar_icon_rect(viewport, &bar);
        self.draw_svg_fit(
            bar.icon_svg_path,
            icon_rect,
            bar.unpin_button.tint,
            bar.unpin_button.size,
        )?;

        let label_rect = minibar::minibar_label_rect(viewport, &bar);
        match app.zones.get(zone_id) {
            Some(zone) if zone.items.is_empty() => {
                self.draw_text("Empty zone", label_rect, bar.unpin_button.tint)?;
            }
            Some(zone) => {
                let capacity = minibar::minibar_item_capacity(viewport, &bar);
                for (index, item) in zone
                    .items
                    .iter()
                    .take(capacity.min(minibar::MINIBAR_SOURCE_MAX_ITEMS))
                    .enumerate()
                {
                    if let Some(item_rect) = minibar::minibar_item_rect(viewport, &bar, index) {
                        self.fill_rounded_rect(
                            item_rect,
                            bar.unpin_button.hover_background,
                            BorderRadius::all(8.0),
                        )?;
                        // M2 R4 (2026-05-29) — try the REAL extracted icon
                        // bitmap first (mirrors `draw_item_card`'s branch at
                        // ~2025). Only when the cache misses / decode fails do
                        // we fall back to the extension-derived emoji glyph.
                        // RC-4 Gap 1 — the 32×32 capsule is far too narrow for
                        // a full file name (the old "ite ite ite" symptom);
                        // the capsule is a glance affordance, the full name
                        // lives in the tray.
                        let icon_rect = bento_nano_style::Rect {
                            x: item_rect.x + 4.0,
                            y: item_rect.y + 4.0,
                            width: (item_rect.width - 8.0).max(0.0),
                            height: (item_rect.height - 8.0).max(0.0),
                        };
                        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect)? {
                            let glyph = item_icon::fallback_emoji_for(item.path.as_ref());
                            self.draw_text(glyph.as_str(), icon_rect, bar.unpin_button.tint)?;
                        }
                    }
                }
            }
            None => {
                self.draw_text(bar.label.as_str(), label_rect, bar.unpin_button.tint)?;
            }
        }

        let unpin_rect = minibar::minibar_unpin_rect(viewport, &bar);
        self.draw_svg_fit(
            bar.unpin_button.svg_path,
            unpin_rect,
            bar.unpin_button.tint,
            bar.unpin_button.size,
        )?;
        Ok(())
    }

    fn draw_zone_editor_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let chrome = zone_editor_geometry::ZoneEditorChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = zone_editor_geometry::zone_editor_panel(viewport);
        let shadow_rect =
            zone_editor_geometry::zone_editor_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 28.0,
        };
        // M6c — zone editor panel title (`h2`).
        self.draw_text_chromatic_title(
            "Edit zone",
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 60.0,
            width: panel.width - 36.0,
            height: 22.0,
        };
        self.draw_text("Name", label_rect, chrome.muted_color)?;

        let input_rect = zone_editor_geometry::zone_editor_name_input_rect(viewport);
        self.fill_rounded_rect(input_rect, chrome.accent_color, chrome.input_radius)?;
        self.fill_rounded_rect(
            inset_rect(input_rect, 2.0),
            chrome.input_background,
            chrome.input_inner_radius,
        )?;

        let session = app.zone_editor.borrow();
        let draft = session
            .as_ref()
            .map(|s| s.draft_name.as_str())
            .unwrap_or("No zone selected");
        let draft_rect = bento_nano_style::Rect {
            x: input_rect.x + 12.0,
            y: input_rect.y + 9.0,
            width: input_rect.width - 24.0,
            height: 24.0,
        };
        self.draw_text(draft, draft_rect, chrome.body_color)?;

        let icon_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 146.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text("Icon", icon_label_rect, chrome.muted_color)?;
        let icon_chip_rect = zone_editor_geometry::zone_editor_icon_rect(viewport);
        self.fill_rounded_rect(icon_chip_rect, chrome.input_background, chrome.row_radius)?;
        let icon_value_rect = bento_nano_style::Rect {
            x: icon_chip_rect.x + 10.0,
            y: icon_chip_rect.y + 4.0,
            width: icon_chip_rect.width - 20.0,
            height: icon_chip_rect.height - 8.0,
        };
        let accent_value = session
            .as_ref()
            .and_then(|s| s.draft_accent_color.as_deref())
            .unwrap_or("None");
        let icon_value = session
            .as_ref()
            .map(|s| s.draft_icon.as_str())
            .unwrap_or("folder");
        self.draw_text(icon_value, icon_value_rect, chrome.body_color)?;

        let accent_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 182.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text("Accent", accent_label_rect, chrome.muted_color)?;
        let accent_row_rect = zone_editor_geometry::zone_editor_accent_rect(viewport);
        self.fill_rounded_rect(accent_row_rect, chrome.input_background, chrome.row_radius)?;
        let accent_swatch_rect = zone_editor_geometry::zone_editor_accent_swatch_rect(viewport);
        self.fill_rounded_rect(
            accent_swatch_rect,
            chrome.input_background,
            chrome.swatch_radius,
        )?;
        if let Some(color) = session
            .as_ref()
            .and_then(|s| s.draft_accent_color.as_deref())
            .and_then(parse_hex_color)
        {
            self.fill_rounded_rect(
                inset_rect(accent_swatch_rect, 3.0),
                color,
                chrome.swatch_inner_radius,
            )?;
        }
        let accent_value_rect = bento_nano_style::Rect {
            x: accent_row_rect.x + 36.0,
            y: accent_row_rect.y + 4.0,
            width: accent_row_rect.width - 46.0,
            height: 18.0,
        };
        self.draw_text(accent_value, accent_value_rect, chrome.body_color)?;

        let grid_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 218.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text("Grid", grid_label_rect, chrome.muted_color)?;
        let grid_value_rect = zone_editor_geometry::zone_editor_grid_rect(viewport);
        self.fill_rounded_rect(grid_value_rect, chrome.input_background, chrome.row_radius)?;
        let grid_text_rect = inset_rect(grid_value_rect, 5.0);
        let grid_value = session
            .as_ref()
            .map(|s| grid_columns_label(s.draft_grid_columns))
            .unwrap_or("4 columns");
        self.draw_text(grid_value, grid_text_rect, chrome.body_color)?;

        let capsule_size_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 244.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text("Size", capsule_size_label_rect, chrome.muted_color)?;
        let capsule_size_rect = zone_editor_geometry::zone_editor_capsule_size_rect(viewport);
        self.fill_rounded_rect(
            capsule_size_rect,
            chrome.input_background,
            chrome.row_radius,
        )?;
        let capsule_size_text_rect = inset_rect(capsule_size_rect, 5.0);
        let capsule_size = session
            .as_ref()
            .map(|s| s.draft_capsule_size.as_str())
            .unwrap_or("medium");
        self.draw_text(capsule_size, capsule_size_text_rect, chrome.body_color)?;

        let capsule_shape_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 270.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text("Shape", capsule_shape_label_rect, chrome.muted_color)?;
        let capsule_shape_rect = zone_editor_geometry::zone_editor_capsule_shape_rect(viewport);
        self.fill_rounded_rect(
            capsule_shape_rect,
            chrome.input_background,
            chrome.row_radius,
        )?;
        let capsule_shape_text_rect = inset_rect(capsule_shape_rect, 5.0);
        let capsule_shape = session
            .as_ref()
            .map(|s| s.draft_capsule_shape.as_str())
            .unwrap_or("pill");
        self.draw_text(capsule_shape, capsule_shape_text_rect, chrome.body_color)?;

        let hint_rect = zone_editor_geometry::zone_editor_hint_rect(viewport);
        self.draw_text(
            "Type name; click Icon for picker; F2/F3/F4/F5 quick-cycle.",
            hint_rect,
            chrome.muted_color,
        )?;
        let save_rect = zone_editor_geometry::zone_editor_save_rect(viewport);
        self.fill_rounded_rect(save_rect, chrome.accent_color, chrome.button_radius)?;
        let save_text = inset_rect(save_rect, 4.0);
        self.draw_text("Save", save_text, chrome.body_color)?;
        let cancel_rect = zone_editor_geometry::zone_editor_cancel_rect(viewport);
        self.fill_rounded_rect(cancel_rect, chrome.input_background, chrome.button_radius)?;
        let cancel_text = inset_rect(cancel_rect, 4.0);
        self.draw_text("Cancel", cancel_text, chrome.body_color)?;
        Ok(())
    }

    fn draw_item_file_rename_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let chrome = item_file_rename_geometry::ItemFileRenameChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = item_file_rename_geometry::item_file_rename_panel_rect(viewport);
        let shadow_rect = item_file_rename_geometry::item_file_rename_panel_shadow_rect(
            panel,
            chrome.panel_shadow,
        );
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;

        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 26.0,
        };
        // M6c — file rename panel title (`h2`).
        self.draw_text_chromatic_title(
            "Rename file",
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.item_file_rename.borrow();
        let current_path = session
            .as_ref()
            .map(|entry| entry.current_path.as_str())
            .unwrap_or("No item selected");
        let path_rect = item_file_rename_geometry::item_file_rename_path_rect(viewport);
        self.draw_text(current_path, path_rect, chrome.muted_color)?;

        let label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 84.0,
            width: panel.width - 36.0,
            height: 18.0,
        };
        self.draw_text("New file name", label_rect, chrome.muted_color)?;

        let input_rect = item_file_rename_geometry::item_file_rename_input_rect(viewport);
        self.fill_rounded_rect(input_rect, chrome.accent_color, chrome.input_radius)?;
        self.fill_rounded_rect(
            inset_rect(input_rect, 2.0),
            chrome.input_background,
            chrome.input_inner_radius,
        )?;
        let draft = session
            .as_ref()
            .map(|entry| entry.draft_name.as_str())
            .unwrap_or("");
        let draft_rect = bento_nano_style::Rect {
            x: input_rect.x + 12.0,
            y: input_rect.y + 9.0,
            width: input_rect.width - 24.0,
            height: 20.0,
        };
        self.draw_text(draft, draft_rect, chrome.body_color)?;

        let status = session
            .as_ref()
            .and_then(|entry| entry.status.as_ref())
            .map(|text| (text.as_str(), chrome.error_color))
            .unwrap_or(("Enter to rename; Esc to cancel.", chrome.muted_color));
        let status_rect = item_file_rename_geometry::item_file_rename_status_rect(viewport);
        self.draw_text(status.0, status_rect, status.1)?;
        Ok(())
    }

    fn draw_icon_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = icon_picker::IconPickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = picker_geometry::picker_panel(viewport);
        let shadow_rect = picker_geometry::picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 28.0,
        };
        // M6c — icon picker panel title (`h2`).
        self.draw_text_chromatic_title(
            "Icon picker",
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.icon_picker.borrow();
        let selected_icon = session
            .as_ref()
            .map(|s| s.selected_icon.as_str())
            .unwrap_or("No selection");
        let target_label = match session.as_ref().and_then(|s| s.zone_id) {
            Some(_) => "Target: zone icon",
            None if session.is_some() => "Target: BulkManager selection",
            None => "Target: none",
        };

        let target_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 58.0,
            width: panel.width - 36.0,
            height: 22.0,
        };
        self.draw_text(target_label, target_rect, chrome.muted_color)?;

        let chip_rect = picker_geometry::icon_picker_selected_rect(viewport);
        self.fill_rounded_rect(chip_rect, chrome.accent_color, chrome.chip_radius)?;
        self.fill_rounded_rect(
            inset_rect(chip_rect, 2.0),
            chrome.chip_background,
            chrome.chip_inner_radius,
        )?;
        let selected_rect = bento_nano_style::Rect {
            x: chip_rect.x + 12.0,
            y: chip_rect.y + 10.0,
            width: chip_rect.width - 24.0,
            height: 24.0,
        };
        self.draw_text(selected_icon, selected_rect, chrome.body_color)?;

        for (index, kind) in ALL_ICON_KINDS.iter().enumerate() {
            let slot_rect = picker_geometry::icon_picker_slot_rect(viewport, index);
            let selected = kind.matches_wire(selected_icon);
            let border_color = if selected {
                chrome.accent_color
            } else {
                chrome.chip_background
            };
            self.fill_rounded_rect(slot_rect, border_color, chrome.slot_radius)?;
            self.fill_rounded_rect(
                inset_rect(slot_rect, 2.0),
                chrome.chip_background,
                chrome.slot_inner_radius,
            )?;
            let icon_rect = bento_nano_style::Rect {
                x: slot_rect.x + (slot_rect.width - 24.0) * 0.5,
                y: slot_rect.y + 7.0,
                width: 24.0,
                height: 24.0,
            };
            self.draw_svg_document_stroke_fit(
                kind.source_svg(),
                icon_rect,
                chrome.body_color,
                24.0,
            )?;
            let slug_rect = bento_nano_style::Rect {
                x: slot_rect.x + 8.0,
                y: slot_rect.y + 37.0,
                width: slot_rect.width - 16.0,
                height: 18.0,
            };
            self.draw_text(kind.as_str(), slug_rect, chrome.body_color)?;
        }

        let hint_rect = picker_geometry::icon_picker_hint_rect(viewport, ALL_ICON_KINDS.len());
        self.draw_text(
            "Click an icon to save. F2 or Right cycles icon. Enter saves. Esc cancels.",
            hint_rect,
            chrome.muted_color,
        )?;
        if session.is_none() {
            let warning_rect = bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 184.0,
                width: panel.width - 36.0,
                height: 24.0,
            };
            self.draw_text(
                "Open from a zone to commit the selected icon.",
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }

    fn draw_palette_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = palette_picker::PalettePickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = picker_geometry::picker_panel(viewport);
        let shadow_rect = picker_geometry::picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 28.0,
        };
        // M6c — palette picker panel title (`h2`).
        self.draw_text_chromatic_title(
            "Palette picker",
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.palette_picker.borrow();
        let target_label = match session.as_ref().map(|s| s.target) {
            Some(PaletteTarget::ZoneAccent(_)) => "Target: zone accent",
            Some(PaletteTarget::ThemeBase) => "Target: theme base accent",
            Some(PaletteTarget::BulkManagerSelectedAccent) => "Target: BulkManager selection",
            None => "Target: none",
        };
        let selected_accent = session
            .as_ref()
            .and_then(|s| s.selected_accent.as_deref())
            .unwrap_or("None");

        let target_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 58.0,
            width: panel.width - 36.0,
            height: 22.0,
        };
        self.draw_text(target_label, target_rect, chrome.muted_color)?;

        let selected = session.as_ref().and_then(|s| s.selected_accent.as_deref());
        for (index, swatch) in palette_picker::swatch_table().iter().enumerate() {
            let swatch_rect = picker_geometry::palette_picker_swatch_rect(viewport, index);
            let is_selected = selected == Some(swatch.hex.as_str());
            let border = if is_selected {
                chrome.warning_color
            } else {
                chrome.chip_background
            };
            self.fill_rounded_rect(swatch_rect, border, chrome.swatch_radius)?;
            if let Some(color) = parse_hex_color(swatch.hex.as_str()) {
                self.fill_rounded_rect(
                    inset_rect(swatch_rect, 3.0),
                    color,
                    chrome.swatch_inner_radius,
                )?;
            }
        }
        let clear_rect = picker_geometry::palette_picker_clear_rect(viewport);
        let clear_border = if selected.is_none() {
            chrome.warning_color
        } else {
            chrome.chip_background
        };
        self.fill_rounded_rect(clear_rect, clear_border, chrome.clear_radius)?;
        self.fill_rounded_rect(
            inset_rect(clear_rect, 2.0),
            chrome.chip_background,
            chrome.clear_inner_radius,
        )?;
        let clear_text_rect = bento_nano_style::Rect {
            x: clear_rect.x + 8.0,
            y: clear_rect.y + 5.0,
            width: clear_rect.width - 16.0,
            height: 20.0,
        };
        self.draw_text("Clear", clear_text_rect, chrome.body_color)?;

        let value_rect = picker_geometry::palette_picker_value_rect(viewport);
        self.draw_text(selected_accent, value_rect, chrome.body_color)?;

        let hint_rect = picker_geometry::palette_picker_hint_rect(viewport);
        self.draw_text(
            "Click a swatch or Clear to save. F3/Right cycles. Esc cancels.",
            hint_rect,
            chrome.muted_color,
        )?;
        if session.as_ref().map(|s| s.target).is_none() {
            let warning_rect = bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 184.0,
                width: panel.width - 36.0,
                height: 24.0,
            };
            self.draw_text(
                "No palette target is active.",
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }

    fn draw_capsule_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave D: consume Wave B Tauri-token SSoT.
        let chrome = capsule_picker::CapsulePickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = capsule_picker::capsule_picker_panel_rect(viewport);
        let shadow_rect =
            capsule_picker::capsule_picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — capsule picker panel title (`h2`).
        self.draw_text_chromatic_title(
            "Context Capsules",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.draw_text(
            "C captures current zones. Enter/R restores. Del/D deletes. Up/Down selects.",
            capsule_picker::capsule_picker_hint_rect(viewport),
            chrome.muted_color,
        )?;

        let state = app.capsule_picker.borrow();
        if let Some(error) = state.last_error() {
            self.draw_text(
                error,
                capsule_picker::capsule_picker_error_rect(viewport),
                chrome.error_color,
            )?;
        }
        if state.entries().is_empty() {
            self.draw_text(
                "No capsules yet. Press C to capture the current selected-stack layout.",
                capsule_picker::capsule_picker_empty_rect(viewport),
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (index, entry) in state
            .entries()
            .iter()
            .take(capsule_picker::CAPSULE_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = capsule_picker::capsule_picker_row_rect(viewport, index);
            let bg = if index == state.selected_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            self.draw_text(
                entry.name.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 5.0,
                    width: row.width - 20.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            self.draw_text(
                entry.captured_at.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 22.0,
                    width: row.width - 20.0,
                    height: 16.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    fn draw_bulk_manager_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: Tauri SSoT tokens for the BulkManager panel.
        use bento_nano_style::tokens as style_tokens;
        let chrome = bulk_manager_panel::BulkManagerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = bulk_manager_panel::bulk_manager_panel_rect(viewport);
        let search_rect = bulk_manager_panel::bulk_manager_search_rect(viewport);
        let shadow_rect =
            bulk_manager_panel::bulk_manager_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — bulk manager panel title (`h2`).
        self.draw_text_chromatic_title(
            "Bulk Manager",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: (search_rect.x - panel.x - 30.0).max(160.0),
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        // RC-4 Gap 3 — split the long helper into two lines so it never
        // overpaints the status row below it. The original single-line
        // version overflowed `panel.width - 36` at any reasonable font
        // size, wrapped to 2 visual rows, and clashed with the status
        // text at `panel.y + 80`.
        let bulk_line_height = style_tokens::TYPOGRAPHY.sm.size_px
            * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            "F/click Search then type to filter · Up/Down cursor · Space select · A all · I invert",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 52.0,
                width: panel.width - 36.0,
                height: bulk_line_height,
            },
            chrome.muted_color,
        )?;
        self.draw_text(
            "H hide · S show · G/R/C/P/O layout · U metadata · T text · D delete · M move",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 52.0 + bulk_line_height + style_tokens::SPACING.xs,
                width: panel.width - 36.0,
                height: bulk_line_height,
            },
            chrome.muted_color,
        )?;

        let state = app.bulk_manager.borrow();
        let search_fill = if state.search_focused() {
            chrome.cursor_background
        } else {
            chrome.row_background
        };
        self.fill_rounded_rect(search_rect, search_fill, chrome.search_radius)?;
        let search_body = if state.search().is_empty() {
            "Search zones..."
        } else {
            state.search()
        };
        let search_text = smol_str::SmolStr::new(format!("Search: {search_body}"));
        self.draw_text(
            search_text.as_str(),
            bento_nano_style::Rect {
                x: search_rect.x + 10.0,
                y: search_rect.y + 7.0,
                width: search_rect.width - 20.0,
                height: 18.0,
            },
            chrome.body_color,
        )?;
        let rows = state.visible_rows();
        let row_window_start =
            bulk_manager_panel::bulk_manager_visible_window_start(state.cursor_index(), rows.len());
        let row_window_summary =
            bulk_manager_panel::bulk_manager_visible_window_summary(row_window_start, rows.len());
        let selected_count = state.selected().len();
        let base_status_text = app.bulk_manager_status.borrow().clone().unwrap_or_else(|| {
            smol_str::SmolStr::new(format!(
                "{} zones listed, {} selected",
                rows.len(),
                selected_count
            ))
        });
        let status_text = if let Some(summary) = row_window_summary {
            smol_str::SmolStr::new(format!("{base_status_text} — {summary}"))
        } else {
            base_status_text
        };
        // RC-4 Gap 3 — status row sits below the 2 helper lines (52 +
        // 2*line_height + xs gap). The legacy `panel.y + 80.0` baseline
        // pre-dated the helper split and now clashes with helper line 2.
        let status_top = (panel.y
            + 52.0
            + bulk_line_height * 2.0
            + style_tokens::SPACING.xs * 2.0)
            .max(panel.y + 80.0);
        self.draw_text(
            status_text.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: status_top,
                width: panel.width - 36.0,
                height: bulk_line_height,
            },
            chrome.muted_color,
        )?;
        for spec in bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS {
            let rect = bulk_manager_panel::bulk_manager_button_rect(viewport, *spec);
            self.fill_rounded_rect(rect, chrome.row_background, chrome.button_radius)?;
            // RC-4 Gap 3 — `draw_text_no_wrap` keeps the 4-letter button
            // labels ("Show", "Move", "Close") on a single line and trims
            // with an ellipsis if the layout box is too narrow, instead of
            // wrapping them into "Sho/w", "Mov", "Clos/e" against the wide
            // YaHei UI fallback Latin metrics. Shrink the horizontal pad
            // from 7 px to SPACING.xs (4 px) each side to give the run an
            // extra 6 px of room — enough for every label in the table to
            // measure clean at the spec'd width without column changes.
            self.draw_text_no_wrap(
                spec.label,
                bento_nano_style::Rect {
                    x: rect.x + style_tokens::SPACING.xs,
                    y: rect.y + 3.0,
                    width: rect.width - style_tokens::SPACING.xs * 2.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;
        }
        for key in bulk_manager_panel::SortKey::ALL {
            let rect = bulk_manager_panel::bulk_manager_sort_header_rect(viewport, *key);
            let active = state.sort_key() == *key;
            let fill = if active {
                chrome.cursor_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(rect, fill, chrome.sort_radius)?;
            let suffix = if active {
                match state.sort_direction() {
                    bulk_manager_panel::SortDirection::Ascending => " ↑",
                    bulk_manager_panel::SortDirection::Descending => " ↓",
                }
            } else {
                ""
            };
            let label = smol_str::SmolStr::new(format!("{}{}", key.label(), suffix));
            // RC-4 Gap 3 — same no-wrap protection as the action buttons.
            self.draw_text_no_wrap(
                label.as_str(),
                bento_nano_style::Rect {
                    x: rect.x + style_tokens::SPACING.xs,
                    y: rect.y + 2.0,
                    width: rect.width - style_tokens::SPACING.xs * 2.0,
                    height: 14.0,
                },
                chrome.body_color,
            )?;
        }
        if let Some(edit) = state.text_edit() {
            let draft = if edit.draft.is_empty() {
                edit.field.placeholder()
            } else {
                edit.draft.as_str()
            };
            let edit_text = smol_str::SmolStr::new(format!(
                "Text edit {}: {}    F2 field | Enter apply | Backspace edit | Esc cancel",
                edit.field.label(),
                draft
            ));
            self.fill_rounded_rect(
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + 158.0,
                    width: panel.width - 36.0,
                    height: 16.0,
                },
                chrome.cursor_background,
                chrome.edit_radius,
            )?;
            self.draw_text(
                edit_text.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + 26.0,
                    y: panel.y + 159.0,
                    width: panel.width - 52.0,
                    height: 14.0,
                },
                chrome.body_color,
            )?;
        }

        if rows.is_empty() {
            self.draw_text(
                "No zones available for bulk operations.",
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + bulk_manager_panel::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (display_index, row_data) in rows
            .iter()
            .skip(row_window_start)
            .take(bulk_manager_panel::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let index = row_window_start + display_index;
            let row = bulk_manager_panel::bulk_manager_row_rect(viewport, display_index);
            let bg = if state.is_selected(row_data.id) {
                chrome.selected_background
            } else if index == state.cursor_index() {
                chrome.cursor_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let marker = if state.is_selected(row_data.id) {
                "[x]"
            } else if index == state.cursor_index() {
                "[>]"
            } else {
                "[ ]"
            };
            let text = smol_str::SmolStr::new(format!(
                "{} {}  state={}  lock={}  mode={}  icon={}  cap={}  items={}  size={}x{}%  pos={},{}%",
                marker,
                row_data.display_name,
                if row_data.visible {
                    "visible"
                } else {
                    "hidden"
                },
                if row_data.locked { "on" } else { "off" },
                row_data.display_mode,
                row_data.icon_slug,
                row_data.capsule_size,
                row_data.item_count,
                row_data.width_percent,
                row_data.height_percent,
                row_data.position_x_percent,
                row_data.position_y_percent
            ));
            self.draw_text(
                text.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 7.0,
                    width: row.width - 20.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
        }
        Ok(())
    }

    fn draw_timeline_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: Tauri SSoT tokens for the Timeline panel.
        let chrome = timeline_panel::TimelinePanelChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = timeline_panel::timeline_panel_rect(viewport);
        let shadow_rect = timeline_panel::timeline_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — timeline panel title (`h2`).
        self.draw_text_chromatic_title(
            "Desktop Timeline",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.draw_text(
            "Click rows to preview. Buttons save/pin/restore/delete. Ctrl+Z / Ctrl+Shift+Z undo-redo. Esc closes.",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.muted_color,
        )?;

        let state = app.timeline_panel.borrow();
        let status = if let Some(error) = state.error() {
            smol_str::SmolStr::new(format!("Error: {error}"))
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(format!("Loaded {} checkpoints", state.entries().len()))
        };
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 80.0,
                width: panel.width - 36.0,
                height: 22.0,
            },
            if state.error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        for spec in timeline_panel::TIMELINE_ACTION_BUTTONS {
            let rect = timeline_panel::timeline_button_rect(viewport, *spec);
            self.fill_rounded_rect(rect, chrome.action_background, chrome.button_radius)?;
            self.draw_text(
                spec.label,
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 6.0,
                    width: rect.width - 16.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;
        }

        if state.entries().is_empty() {
            self.draw_text(
                "No checkpoints yet. Click Save or press S to save the current selected-stack layout.",
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + timeline_panel::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        let list_w = panel.width * 0.56;
        for (index, entry) in state
            .entries()
            .iter()
            .take(timeline_panel::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = timeline_panel::timeline_row_rect(viewport, index);
            let bg = if index == state.cursor_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let pin = if entry.pinned { "★" } else { " " };
            let line = smol_str::SmolStr::new(format!(
                "{pin} {}  zones={} items={}",
                entry.captured_at, entry.zone_count, entry.item_count
            ));
            self.draw_text(
                line.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 4.0,
                    width: row.width - 20.0,
                    height: 17.0,
                },
                chrome.body_color,
            )?;
            let delta = if entry.delta_summary.is_empty() {
                "no change"
            } else {
                entry.delta_summary.as_str()
            };
            self.draw_text(
                delta,
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 21.0,
                    width: row.width - 20.0,
                    height: 15.0,
                },
                chrome.muted_color,
            )?;
        }

        let detail_x = panel.x + list_w + 12.0;
        let detail_w = panel.width - (detail_x - panel.x) - 18.0;
        if let Some(active) = state.active() {
            let detail = smol_str::SmolStr::new(format!(
                "Selected {}\ntrigger={} pinned={} zones={} captured={}",
                active.id,
                active.trigger,
                active.pinned,
                active.snapshot.zones.len(),
                active.snapshot.captured_at
            ));
            self.draw_text(
                detail.as_str(),
                bento_nano_style::Rect {
                    x: detail_x,
                    y: panel.y + timeline_panel::RUNTIME_ROW_TOP_PX,
                    width: detail_w,
                    height: 72.0,
                },
                chrome.body_color,
            )?;
            let thumbnail_rect = timeline_detail_thumbnail_rect(panel, detail_x, detail_w);
            // Wave E: Tauri SSoT tokens for the inline snapshot thumbnail.
            let thumbnail_chrome = snapshot_picker::SnapshotThumbnailChrome::from_tauri_tokens(
                app.active_theme_tauri(),
                app.active_theme_radius_tauri(),
            );
            self.draw_snapshot_thumbnail(&active.snapshot, thumbnail_rect, thumbnail_chrome)?;
        }
        Ok(())
    }

    fn draw_snapshot_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: Tauri SSoT tokens for the Snapshot picker panel.
        use bento_nano_style::tokens as style_tokens;
        let chrome = snapshot_picker::SnapshotPickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = snapshot_picker::snapshot_picker_panel_rect(viewport);
        let shadow_rect =
            snapshot_picker::snapshot_picker_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — snapshot picker panel title (`h2`).
        self.draw_text_chromatic_title(
            "Layout Snapshots",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let helper_line_h = style_tokens::TYPOGRAPHY.sm.size_px
            * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            "Click rows to select. Buttons save/load/delete/open Timeline.",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0,
                width: panel.width - 36.0,
                height: helper_line_h,
            },
            chrome.muted_color,
        )?;
        self.draw_text(
            "D confirms delete. Esc closes.",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0 + helper_line_h,
                width: panel.width - 36.0,
                height: helper_line_h,
            },
            chrome.muted_color,
        )?;

        let state = app.snapshot_picker.borrow();
        let status = if let Some(error) = state.error() {
            smol_str::SmolStr::new(format!("Error: {error}"))
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(format!("Loaded {} snapshots", state.entries().len()))
        };
        let status_y = panel.y + 50.0 + helper_line_h * 2.0 + style_tokens::SPACING.xs;
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: status_y,
                width: panel.width - 36.0,
                height: 22.0,
            },
            if state.error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        for spec in snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS {
            let rect = snapshot_picker::snapshot_picker_button_rect(viewport, *spec);
            self.fill_rounded_rect(rect, chrome.action_background, chrome.button_radius)?;
            self.draw_text_no_wrap(
                spec.label,
                bento_nano_style::Rect {
                    x: rect.x + style_tokens::SPACING.xs,
                    y: rect.y + 6.0,
                    width: rect.width - style_tokens::SPACING.xs * 2.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;
        }

        if state.entries().is_empty() {
            self.draw_text(
                "No snapshots yet. Click Save or press S to save the current selected-stack layout.",
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + snapshot_picker::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (index, snapshot) in state
            .entries()
            .iter()
            .take(snapshot_picker::RUNTIME_VISIBLE_ROW_LIMIT)
            .enumerate()
        {
            let row = snapshot_picker::snapshot_picker_row_rect(viewport, index);
            let bg = if index == state.cursor_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, bg, chrome.row_radius)?;
            let preview_rect = snapshot_row_preview_rect(row);
            self.draw_snapshot_thumbnail(snapshot, preview_rect, chrome.thumbnail_chrome)?;
            let title = if snapshot.name.trim().is_empty() {
                snapshot.id.as_str()
            } else {
                snapshot.name.as_str()
            };
            let text_width = (preview_rect.x - row.x - 22.0).max(48.0);
            self.draw_text(
                title,
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 4.0,
                    width: text_width,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            let meta = snapshot_picker::meta_line(snapshot, snapshot.captured_at.as_str(), "Zones");
            let confirm = state.row_action().is_awaiting_for(snapshot.id.as_str());
            let meta_text = if confirm {
                smol_str::SmolStr::new(format!("{meta}  •  Confirm delete with D"))
            } else {
                meta
            };
            self.draw_text(
                meta_text.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 24.0,
                    width: text_width,
                    height: 16.0,
                },
                if confirm {
                    chrome.error_color
                } else {
                    chrome.muted_color
                },
            )?;
        }
        Ok(())
    }

    fn draw_snapshot_thumbnail(
        &mut self,
        snapshot: &DesktopSnapshot,
        rect: bento_nano_style::Rect,
        chrome: snapshot_picker::SnapshotThumbnailChrome,
    ) -> Result<(), RenderError> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        self.fill_rounded_rect(rect, chrome.border_color, chrome.border_radius)?;
        let content_bg = inset_rect(rect, 1.0);
        self.fill_rounded_rect(content_bg, chrome.background_color, chrome.content_radius)?;

        let mut drew_any = false;
        for zone in &snapshot.zones {
            let Some(zone_rect) = snapshot_zone_thumbnail_rect(zone, rect) else {
                continue;
            };
            let fill = zone
                .accent_color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or(chrome.fallback_zone_color);
            self.fill_rounded_rect(zone_rect, fill, chrome.zone_radius)?;
            drew_any = true;
        }

        if !drew_any {
            self.draw_text("No zones", inset_rect(rect, 8.0), chrome.empty_text_color)?;
        }
        Ok(())
    }

    fn draw_rules_wizard_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let chrome = rules_wizard::RulesWizardChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = rules_wizard::rules_wizard_panel_rect(viewport);
        let shadow_rect = rules_wizard::rules_wizard_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — rules wizard panel title (`h2`).
        self.draw_text_chromatic_title(
            "Rules Wizard",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 36.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.draw_text(
            "Type edits current step. Click buttons or use F2/F3/F4, Enter, E/R/D, Esc.",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 50.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.muted_color,
        )?;

        let wizard = app.rules_wizard.borrow();
        let rules = app.rules_wizard_rules.borrow();
        let cursor = app.rules_wizard_rule_cursor.get();
        let rule_window_start =
            rules_wizard::rules_wizard_visible_rule_window_start(cursor, rules.len());
        let rule_window_summary =
            rules_wizard::rules_wizard_visible_rule_summary(rule_window_start, rules.len());
        let status = app.rules_wizard_status.borrow().clone();
        let step = wizard.step();
        let step_line = smol_str::SmolStr::new(format!(
            "Step {}/{}: {}   complete={}   enabled={}",
            step.index(),
            WizardStep::TOTAL,
            wizard_step_label(step),
            wizard.is_complete(),
            wizard.enabled()
        ));
        self.draw_text(
            step_line.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 82.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            chrome.body_color,
        )?;

        let base_status_text = if let Some(error) = wizard.last_error() {
            smol_str::SmolStr::new(format!("Error: {error}"))
        } else if let Some(status) = status {
            status
        } else {
            smol_str::SmolStr::new(format!("Loaded {} persisted rules", rules.len()))
        };
        let status_text = if let Some(summary) = rule_window_summary {
            smol_str::SmolStr::new(format!("{base_status_text} — {summary}"))
        } else {
            base_status_text
        };
        self.draw_text(
            status_text.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 108.0,
                width: panel.width - 36.0,
                height: 24.0,
            },
            if wizard.last_error().is_some() {
                chrome.error_color
            } else {
                chrome.muted_color
            },
        )?;

        for spec in rules_wizard::RULES_WIZARD_ACTION_BUTTONS {
            let rect = rules_wizard::rules_wizard_button_rect(viewport, *spec);
            self.fill_rounded_rect(rect, chrome.action_background, chrome.button_radius)?;
            self.draw_text(
                spec.label,
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 5.0,
                    width: rect.width - 16.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;
        }

        let form_x = panel.x + 18.0;
        let list_x = panel.x + panel.width * 0.54;
        let top = panel.y + rules_wizard::RUNTIME_FORM_TOP_PX;
        let form_w = (panel.width * 0.50).max(260.0);
        let list_w = panel.width - (list_x - panel.x) - 18.0;
        let condition_index = wizard.condition_cursor();
        let condition_count = wizard.conditions().len();
        let condition_window_start = rules_wizard::rules_wizard_visible_condition_window_start(
            condition_index,
            condition_count,
        );
        let condition_window_summary = rules_wizard::rules_wizard_visible_condition_summary(
            condition_window_start,
            condition_count,
        );
        let action = wizard.action();
        let action_text = smol_str::SmolStr::new(format!(
            "Action: {} = {}",
            action_label(action.kind),
            if action.value.trim().is_empty() {
                "<type value>"
            } else {
                action.value.as_str()
            }
        ));
        let name_text = smol_str::SmolStr::new(format!(
            "Name: {}",
            if wizard.name().trim().is_empty() {
                "<type name>"
            } else {
                wizard.name()
            }
        ));
        let run_text = smol_str::SmolStr::new(format!(
            "Run: {}  interval={}m",
            run_mode_label(wizard.run_mode()),
            wizard.interval_minutes()
        ));
        let preview_text = smol_str::SmolStr::new(format!(
            "Preview: {}{} hits",
            if wizard.preview_busy() { "busy, " } else { "" },
            wizard.preview_hits().len()
        ));

        let conditions_heading = if let Some(summary) = condition_window_summary {
            smol_str::SmolStr::new(format!(
                "Conditions [{}] — {summary}",
                combine_label(wizard.combine())
            ))
        } else {
            smol_str::SmolStr::new(format!("Conditions [{}]", combine_label(wizard.combine())))
        };
        self.draw_text(
            conditions_heading.as_str(),
            bento_nano_style::Rect {
                x: form_x,
                y: top,
                width: form_w,
                height: 24.0,
            },
            chrome.title_color,
        )?;
        if condition_count == 0 {
            self.draw_text(
                "No conditions",
                bento_nano_style::Rect {
                    x: form_x,
                    y: top + 32.0,
                    width: form_w,
                    height: 22.0,
                },
                chrome.muted_color,
            )?;
        } else {
            for (display_index, row_index) in (condition_window_start
                ..condition_count
                    .min(condition_window_start + rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT))
                .enumerate()
            {
                let Some(row) = wizard.conditions().get(row_index) else {
                    continue;
                };
                let rect = rules_wizard::rules_wizard_condition_row_rect(viewport, display_index);
                let selected = row_index == condition_index.min(condition_count.saturating_sub(1));
                self.fill_rounded_rect(
                    rect,
                    if selected {
                        chrome.selected_background
                    } else {
                        chrome.row_background
                    },
                    chrome.row_radius,
                )?;
                let text = smol_str::SmolStr::new(format!(
                    "{} {}. {} = {}",
                    if selected { "[>]" } else { "[ ]" },
                    row_index + 1,
                    predicate_label(row.kind),
                    if row.value.trim().is_empty() {
                        "<type value>"
                    } else {
                        row.value.as_str()
                    }
                ));
                self.draw_text(
                    text.as_str(),
                    bento_nano_style::Rect {
                        x: rect.x + 10.0,
                        y: rect.y + 4.0,
                        width: rect.width - 20.0,
                        height: 16.0,
                    },
                    chrome.body_color,
                )?;
            }
        }

        let detail_top = top
            + 44.0
            + rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT as f32
                * rules_wizard::RUNTIME_CONDITION_ROW_STRIDE_PX;
        for (idx, line) in [
            action_text.as_str(),
            preview_text.as_str(),
            name_text.as_str(),
            run_text.as_str(),
        ]
        .iter()
        .enumerate()
        {
            self.draw_text(
                line,
                bento_nano_style::Rect {
                    x: form_x,
                    y: detail_top + idx as f32 * 24.0,
                    width: form_w,
                    height: 20.0,
                },
                chrome.body_color,
            )?;
        }

        self.draw_text(
            "Persisted rules",
            bento_nano_style::Rect {
                x: list_x,
                y: top,
                width: list_w,
                height: 24.0,
            },
            chrome.title_color,
        )?;
        if rules.is_empty() {
            self.draw_text(
                "No rules saved yet. Complete the wizard and press Enter on Review.",
                bento_nano_style::Rect {
                    x: list_x,
                    y: top + 32.0,
                    width: list_w,
                    height: 42.0,
                },
                chrome.muted_color,
            )?;
        } else {
            for (display_index, rule) in rules
                .iter()
                .skip(rule_window_start)
                .take(rules_wizard::RUNTIME_VISIBLE_RULE_LIMIT)
                .enumerate()
            {
                let index = rule_window_start + display_index;
                let row = rules_wizard::rules_wizard_rule_row_rect(viewport, display_index);
                let selected = index == cursor.min(rules.len().saturating_sub(1));
                self.fill_rounded_rect(
                    row,
                    if selected {
                        chrome.selected_background
                    } else {
                        chrome.row_background
                    },
                    chrome.row_radius,
                )?;
                let text = smol_str::SmolStr::new(format!(
                    "{} {}  id={}  enabled={}",
                    if selected { "[>]" } else { "[ ]" },
                    rule.name,
                    rule.id,
                    rule.enabled
                ));
                self.draw_text(
                    text.as_str(),
                    bento_nano_style::Rect {
                        x: row.x + 10.0,
                        y: row.y + 6.0,
                        width: row.width - 20.0,
                        height: 18.0,
                    },
                    chrome.body_color,
                )?;
            }
        }

        for (index, hit) in wizard.preview_hits().iter().take(4).enumerate() {
            let line = smol_str::SmolStr::new(format!("hit: {hit}"));
            self.draw_text(
                line.as_str(),
                bento_nano_style::Rect {
                    x: form_x,
                    y: detail_top + 104.0 + index as f32 * 22.0,
                    width: form_w,
                    height: 18.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    fn draw_search_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: source visual chrome from the Wave B Tauri SSoT
        // (`bento_nano_style::tokens::PALETTE_DARK / RADIUS / SHADOW`) so the
        // selected-stack runtime panels render against the same tokens the
        // Tauri 1.2.4 baseline used. Legacy `from_tokens` constructor is
        // retained for back-compat callers (theme palette mutation tests).
        let chrome = search_bar::SearchBarChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = search_bar::search_panel_rect(viewport);
        let shadow_rect = search_bar::search_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — search panel title (`h2`).
        self.draw_text_chromatic_title(
            "Search",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 110.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close = search_bar::search_close_rect(viewport);
        self.fill_rounded_rect(close, chrome.danger_background, chrome.close_radius)?;
        self.draw_text(
            "Close",
            bento_nano_style::Rect {
                x: close.x + 8.0,
                y: close.y + 5.0,
                width: close.width - 16.0,
                height: 16.0,
            },
            chrome.body_color,
        )?;

        let state = app.search_bar.borrow();
        let input = search_bar::search_input_rect(viewport);
        self.fill_rounded_rect(input, chrome.input_background, chrome.input_radius)?;
        let query_text = if state.query.is_empty() {
            "Type to search zones, files, settings, actions"
        } else {
            state.query.as_str()
        };
        self.draw_text(
            query_text,
            bento_nano_style::Rect {
                x: input.x + 14.0,
                y: input.y + 12.0,
                width: input.width - 28.0,
                height: 24.0,
            },
            if state.query.is_empty() {
                chrome.muted_color
            } else {
                chrome.body_color
            },
        )?;

        let status = app.search_status.borrow().clone().unwrap_or_else(|| {
            smol_str::SmolStr::new_static(
                "Type to search live zones, items, settings, and actions.",
            )
        });
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: input.x,
                y: input.bottom() + 8.0,
                width: input.width,
                height: 22.0,
            },
            chrome.muted_color,
        )?;

        if state.results.is_empty() {
            self.draw_text(
                "No results are currently visible. Results are populated from live AppState only.",
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: input.bottom() + 48.0,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (index, hit) in state
            .results
            .iter()
            .take(search_bar::MAX_VISIBLE_RESULTS)
            .enumerate()
        {
            let row = search_bar::search_row_rect(viewport, index);
            let row_bg = if state.selected_index() == Some(index) {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, row_bg, chrome.row_radius)?;
            self.draw_text(
                hit.icon.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 12.0,
                    y: row.y + 12.0,
                    width: 44.0,
                    height: 20.0,
                },
                chrome.body_color,
            )?;
            self.draw_text(
                hit.name.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 58.0,
                    y: row.y + 6.0,
                    width: row.width - 180.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            self.draw_text(
                hit.breadcrumb.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 58.0,
                    y: row.y + 25.0,
                    width: row.width - 180.0,
                    height: 16.0,
                },
                chrome.muted_color,
            )?;
            let kind_label = match &hit.kind {
                bento_nano_backend::search::SearchItemKind::File => "File",
                bento_nano_backend::search::SearchItemKind::Folder => "Folder",
                bento_nano_backend::search::SearchItemKind::Zone => "Zone",
                bento_nano_backend::search::SearchItemKind::Setting => "Setting",
                bento_nano_backend::search::SearchItemKind::Action => "Action",
            };
            self.draw_text(
                kind_label,
                bento_nano_style::Rect {
                    x: row.right() - 112.0,
                    y: row.y + 14.0,
                    width: 100.0,
                    height: 18.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    fn draw_suggestor_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        // Wave E: Tauri SSoT tokens for the Smart-group suggestor panel.
        // Confidence-badge colours route through the dedicated Tauri tone
        // helper so badges use `accent_green` / `accent_orange` / `text_muted`
        // per Wave A `search-bar-and-suggestor.md`.
        use bento_nano_style::tokens as style_tokens;
        // M6a — live theme palette for the suggestor panel chrome.
        let palette = app.active_theme_tauri();
        let chrome = smart_group_suggestor::SmartGroupSuggestorChrome::from_tauri_tokens(
            palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = smart_group_suggestor::suggestor_panel_rect(viewport);
        let shadow_rect =
            smart_group_suggestor::suggestor_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(panel, chrome.panel_background, chrome.panel_radius)?;
        // M6c — smart-group suggestor panel title (`h2`).
        self.draw_text_chromatic_title(
            "Smart grouping",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 110.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close = smart_group_suggestor::suggestor_close_rect(viewport);
        self.fill_rounded_rect(close, chrome.danger_background, chrome.close_radius)?;
        self.draw_text_no_wrap(
            "Close",
            bento_nano_style::Rect {
                x: close.x + 8.0,
                y: close.y + 5.0,
                width: close.width - 16.0,
                height: 16.0,
            },
            chrome.body_color,
        )?;
        // RC-4 Gap 3 — the long helper string wraps to ≥2 lines at panel
        // width (≈608 px @ 16pt YaHei), and the next status row starts at
        // `RUNTIME_STATUS_TOP_PX = 74` — only 24 px below the helper top.
        // Result: helper line 2 overpaints the status row. Split the
        // helper into two short lines, advance each by one line_height +
        // SPACING.xs, and push the status row below the second helper line.
        let line_height = style_tokens::TYPOGRAPHY.sm.size_px
            * style_tokens::TYPOGRAPHY.sm.line_height;
        let helper_row_advance = line_height + style_tokens::SPACING.xs;
        let helper_top = panel.y + 50.0;
        self.draw_text(
            "Up/Down: suggestion · Left/Right: file · Space: toggle checkbox",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: helper_top,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;
        self.draw_text(
            "A: all · N: none · Enter: apply checked paths",
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: helper_top + helper_row_advance,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;

        let state = app.suggestor.borrow();
        let status = app.suggestor_status.borrow().clone().unwrap_or_else(|| {
            smol_str::SmolStr::new(format!("Loaded {} suggestions", state.entries().len()))
        });
        // Status row sits one line_height below the second helper row so
        // the three messages never share a baseline.
        let status_top = (helper_top + helper_row_advance * 2.0)
            .max(panel.y + smart_group_suggestor::RUNTIME_STATUS_TOP_PX);
        self.draw_text(
            status.as_str(),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: status_top,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;

        if state.entries().is_empty() {
            self.draw_text(
                "No suggestions are available from the current real Desktop scan.",
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: panel.y + smart_group_suggestor::RUNTIME_ROW_TOP_PX,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
            )?;
            return Ok(());
        }

        for (index, entry) in state
            .entries()
            .iter()
            .take(smart_group_suggestor::MAX_VISIBLE_SUGGESTIONS)
            .enumerate()
        {
            let row = smart_group_suggestor::suggestor_row_rect(viewport, index);
            let row_bg = if index == state.selected_index() {
                chrome.selected_background
            } else {
                chrome.row_background
            };
            self.fill_rounded_rect(row, row_bg, chrome.row_radius)?;
            self.draw_text(
                entry.suggestion.icon.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 12.0,
                    y: row.y + 15.0,
                    width: smart_group_suggestor::ROW_ICON_SIZE_PX,
                    height: 22.0,
                },
                chrome.body_color,
            )?;
            let apply = smart_group_suggestor::suggestor_apply_rect(viewport, index);
            let dismiss = smart_group_suggestor::suggestor_dismiss_rect(viewport, index);
            let badge = bento_nano_style::Rect {
                x: apply.x - 104.0,
                y: row.y + 17.0,
                width: 94.0,
                height: 24.0,
            };
            // Wave F carry-over #2: title must respect badge's left edge.
            // Drop the .max(96.0) floor so we never paint into the badge;
            // route through no-wrap so an over-wide title is character-trimmed
            // inside its box instead of stamping a fragment across the badge.
            let text_width = (badge.x - (row.x + 50.0) - 12.0).max(0.0);
            self.draw_text_no_wrap(
                entry.suggestion.name.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 7.0,
                    width: text_width,
                    height: 19.0,
                },
                chrome.body_color,
            )?;
            let summary = smart_group_suggestor::rule_summary(&entry.suggestion);
            let meta = smol_str::SmolStr::new(format!(
                "{}/{} selected - {}",
                entry.selected_path_count(),
                entry.total_path_count(),
                summary
            ));
            self.draw_text_no_wrap(
                meta.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 31.0,
                    width: text_width,
                    height: 17.0,
                },
                chrome.muted_color,
            )?;

            let tone = smart_group_suggestor::confidence_tone(entry.suggestion.confidence);
            let (badge_bg, badge_text) =
                smart_group_suggestor::tone_colors_from_tauri_palette(tone, palette);
            self.fill_rounded_rect(badge, badge_bg, chrome.badge_radius)?;
            let confidence = smol_str::SmolStr::new(format!(
                "{} {}%",
                tone.label(),
                (entry.suggestion.confidence * 100.0).round() as i32
            ));
            self.draw_text(
                confidence.as_str(),
                bento_nano_style::Rect {
                    x: badge.x + 7.0,
                    y: badge.y + 5.0,
                    width: badge.width - 14.0,
                    height: 15.0,
                },
                badge_text,
            )?;

            self.fill_rounded_rect(apply, chrome.action_background, chrome.action_radius)?;
            let apply_text = if state.applying_id() == Some(&entry.id) {
                "Applying"
            } else {
                "Apply"
            };
            self.draw_text_no_wrap(
                apply_text,
                bento_nano_style::Rect {
                    x: apply.x + style_tokens::SPACING.xs,
                    y: apply.y + 5.0,
                    width: apply.width - style_tokens::SPACING.xs * 2.0,
                    height: 15.0,
                },
                chrome.body_color,
            )?;
            self.fill_rounded_rect(dismiss, chrome.danger_background, chrome.action_radius)?;
            self.draw_text_no_wrap(
                "X",
                bento_nano_style::Rect {
                    x: dismiss.x + 10.0,
                    y: dismiss.y + 5.0,
                    width: dismiss.width - 20.0,
                    height: 15.0,
                },
                chrome.body_color,
            )?;
        }

        if let Some(entry) = state.selected_entry() {
            let preview = smart_group_suggestor::suggestor_preview_rect(viewport);
            self.fill_rounded_rect(preview, chrome.preview_background, chrome.preview_radius)?;
            let title = smol_str::SmolStr::new(format!(
                "Manual apply selection: {}/{} checked",
                entry.selected_path_count(),
                entry.total_path_count()
            ));
            self.draw_text(
                title.as_str(),
                bento_nano_style::Rect {
                    x: preview.x + 8.0,
                    y: preview.y + 8.0,
                    width: preview.width - 128.0,
                    height: 16.0,
                },
                chrome.body_color,
            )?;

            let all = smart_group_suggestor::suggestor_select_all_rect(viewport);
            self.fill_rounded_rect(all, chrome.action_background, chrome.preview_button_radius)?;
            self.draw_text_no_wrap(
                "All",
                bento_nano_style::Rect {
                    x: all.x + 12.0,
                    y: all.y + 4.0,
                    width: all.width - 16.0,
                    height: 13.0,
                },
                chrome.body_color,
            )?;
            let none = smart_group_suggestor::suggestor_select_none_rect(viewport);
            self.fill_rounded_rect(none, chrome.action_background, chrome.preview_button_radius)?;
            self.draw_text_no_wrap(
                "None",
                bento_nano_style::Rect {
                    x: none.x + 4.0,
                    y: none.y + 4.0,
                    width: none.width - 8.0,
                    height: 13.0,
                },
                chrome.body_color,
            )?;

            for offset in 0..entry.preview_file_count() {
                let Some(path_index) = entry.preview_path_index(offset) else {
                    continue;
                };
                let Some(path) = entry.suggestion.matching_files.get(path_index) else {
                    continue;
                };
                let rect = smart_group_suggestor::suggestor_preview_file_rect(viewport, offset);
                let focused = path_index == entry.focused_path_index();
                let checked = entry.is_path_selected(path_index);
                let marker = match (focused, checked) {
                    (true, true) => "> [x]",
                    (true, false) => "> [ ]",
                    (false, true) => "  [x]",
                    (false, false) => "  [ ]",
                };
                let label = smol_str::SmolStr::new(format!(
                    "{} {}",
                    marker,
                    smart_group_suggestor::path_basename(path)
                ));
                self.draw_text(
                    label.as_str(),
                    bento_nano_style::Rect {
                        x: rect.x,
                        y: rect.y + 1.0,
                        width: rect.width,
                        height: rect.height,
                    },
                    if checked {
                        chrome.body_color
                    } else {
                        chrome.muted_color
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Borrow the resident D2D context, or return an error when the surface
    /// has been hibernated. All inner draw helpers funnel through this
    /// accessor so the §11 R5 hibernation guard is one-shot, not scattered.
    fn ctx(&self) -> Result<&windows::Win32::Graphics::Direct2D::ID2D1DeviceContext, RenderError> {
        match self.surface.as_ref() {
            Some(s) => Ok(&s.ctx),
            None => Err(RenderError::Platform(
                bento_nano_platform::PlatformError::Init(
                    "Renderer: draw call on hibernated surface (T-099)",
                ),
            )),
        }
    }

    /// Push an axis-aligned D2D clip so subsequent paint is masked to `rect`.
    /// Used by the Settings scrollable body (S-02) so partial rows clip cleanly
    /// at the sticky header/footer edges instead of bleeding past them.
    ///
    /// CRITICAL: every `push_clip` MUST be balanced by exactly one `pop_clip`
    /// before the next `Present` — an unbalanced clip corrupts the device
    /// context. Callers using `?` propagation must capture the clipped paint
    /// into a local and run `pop_clip()` before propagating any error. We use
    /// `D2D1_ANTIALIAS_MODE_ALIASED` (hard pixel edge) so the row/header/footer
    /// boundaries stay crisp; the body band is axis-aligned so there is nothing
    /// to antialias.
    fn push_clip(&self, rect: bento_nano_style::Rect) -> Result<(), RenderError> {
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        let clip = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        // SAFETY: rt valid for the call; `clip` lives until the call returns.
        unsafe {
            rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        }
        Ok(())
    }

    /// Pop the most recent `push_clip`. See `push_clip` for the balancing
    /// contract — leaving a clip pushed corrupts the device context.
    fn pop_clip(&self) -> Result<(), RenderError> {
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; pairs with the matching PushAxisAlignedClip.
        unsafe {
            rt.PopAxisAlignedClip();
        }
        Ok(())
    }

    fn fill_rounded_rect(
        &self,
        rect: bento_nano_style::Rect,
        color: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let brush = self.solid_brush(color)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr lives for the call; brush COM-ref-counted.
        unsafe {
            rt.FillRoundedRectangle(&rr, &brush);
        }
        Ok(())
    }

    /// M6b — paint a multi-layer [`ShadowStack`] under `base` as a simulated
    /// soft fill (the existing grow-and-fill idiom, one fill per layer, no D2D
    /// blur effect on the hot path). Layers draw back-to-front so the inner
    /// surface lift sits under the dominant outer drop. Each layer grows the
    /// rect by `blur + spread`, so `terminal`'s `0 0 0 1px` ring (spread=1,
    /// blur=0) paints a 1-DIP outline and `neo`'s `-6 -6` light extrude shifts
    /// up-left (negative offsets are honoured — the rect simply translates).
    /// An empty stack (`flat`/`brutalism`/`editorial`) is a no-op.
    fn draw_shadow_stack(
        &self,
        base: bento_nano_style::Rect,
        stack: bento_nano_style::ShadowStack,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        for layer in stack.layers() {
            let grow = layer.blur.max(0.0) + layer.spread.max(0.0);
            let rect = bento_nano_style::Rect {
                x: base.x + layer.offset_x - grow,
                y: base.y + layer.offset_y - grow,
                width: base.width + grow * 2.0,
                height: base.height + grow * 2.0,
            };
            self.fill_rounded_rect(rect, layer.color, radius)?;
        }
        Ok(())
    }

    // =========================================================================
    // M6c — the 3 effect render primitives + the post-pass dispatcher.
    //
    // All read `app.active_theme_effect_tauri()` (`Copy`, §10) and no-op for
    // `EffectTauri::None`, so the 14 non-effect themes pay nothing. The blur
    // house-style is alpha-graded grow-and-fill (NOT `CLSID_D2D1Shadow`); no new
    // crate / windows feature (§8). GPU draw itself is verified by the §6 visual
    // smoke — no offscreen unit-test harness exists (§3.4); the pure geometry
    // (`scanline_band_count` / `neon_glow_rect` / `chromatic_split_offsets`) is
    // unit-tested instead.
    // =========================================================================

    /// M6c effect dispatcher — the post-pass effect overlay drawn just before
    /// each `EndDraw` (both the aux-window and main-HWND exits) so it covers
    /// every surface, matching Tauri's `<html>`-level `data-theme-effect`
    /// `::after`. Only `Scanlines` is a full-viewport post-pass; `Neon` is
    /// inline in `draw_zones` and `Chromatic` is inline in the title draws, so
    /// this dispatcher handles ONLY the scanline arm (and no-ops otherwise).
    fn draw_effect_overlay(&self, app: &AppState) -> Result<(), RenderError> {
        if let bento_nano_style::tokens::EffectTauri::Scanlines(scan) =
            app.active_theme_effect_tauri()
        {
            self.draw_scanline_overlay(scan, app.viewport)?;
        }
        Ok(())
    }

    /// M6c scanline (`terminal`) — full-viewport repeating horizontal bands: a
    /// 1-DIP `#00FF9C`@.06 lit stripe every 3 DIP, over the whole `vp`
    /// (`theme-effects.css:6-21`, Tauri `position:fixed; inset:0`). Drawn as a
    /// post-pass overlay above all content (`z-index:9999`).
    ///
    /// **1:1-INTENT divergence (LOCK, §3.1.4)**: Tauri composites the bands with
    /// `mix-blend-mode: overlay`; D2D's enabled-feature primary blend is
    /// source-over, which `fill_rounded_rect` uses here. At α 0.06 over the
    /// near-black terminal surface the two are visually indistinguishable
    /// (overlay only diverges materially over mid-grey, which the terminal theme
    /// has none of). Deliberate intent-parity, NOT byte-parity — same class as
    /// M6b's font substitution. We do NOT enable a D2D blend-effect feature for a
    /// sub-perceptual delta (§8 over-engineering avoidance).
    ///
    /// §10: a stack-`f32` `while` loop of square (`BorderRadius::ZERO`) fills —
    /// no per-band heap alloc; the band count is `ceil(vh/period)`.
    fn draw_scanline_overlay(
        &self,
        scan: bento_nano_style::tokens::ScanlineEffect,
        vp: bento_nano_style::Size,
    ) -> Result<(), RenderError> {
        if scan.color.a <= 0.0
            || vp.width <= 0.0
            || vp.height <= 0.0
            || scan.period_dip <= 0.0
            || scan.band_dip <= 0.0
        {
            return Ok(());
        }
        // `count = ceil(vh / period)` bands at `y = k * period` (the pure helper
        // is the unit-test surface). Indexing `0..count` instead of accumulating
        // a `+= period` float avoids drift on tall viewports.
        let count = scanline_band_count(vp.height, scan.period_dip);
        for k in 0..count {
            let band = bento_nano_style::Rect {
                x: 0.0,
                y: k as f32 * scan.period_dip,
                width: vp.width,
                height: scan.band_dip,
            };
            self.fill_rounded_rect(band, scan.color, BorderRadius::ZERO)?;
        }
        Ok(())
    }

    /// M6c neon (`cyberpunk`) — paint the two-layer `filter: drop-shadow` bloom
    /// behind `base` (`theme-effects.css:23-32`). Reuses the `draw_shadow_stack`
    /// grow-and-fill idiom: each layer grows the rect by its blur (0,0 offset →
    /// symmetric bloom) and fills with the glow colour.
    ///
    /// **ADDITIVE to the M6b `SHADOW_CYBERPUNK` box-shadow** (§1.2 / §3.2.1):
    /// the M6b shadow stack and this `filter` bloom both composite in Tauri with
    /// DIFFERENT blur radii / alphas. Call this AFTER the M6b `draw_shadow_stack`
    /// and BEFORE the surface fill so it layers correctly — do NOT conflate them.
    ///
    /// Draw order (LOCK, §3.2.2): the authored array is `[cyan_inner,
    /// magenta_outer]`; iterating `.rev()` paints the wider magenta (index 1)
    /// FIRST and the tighter brighter cyan (index 0) on TOP, so the bloom reads
    /// cyan-cored with a magenta halo. §10: 2 grown fills, zero alloc; no-op when
    /// a layer's alpha is 0.
    fn draw_neon_glow(
        &self,
        base: bento_nano_style::Rect,
        layers: [bento_nano_style::Shadow; 2],
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        for layer in layers.iter().rev() {
            if layer.color.a <= 0.0 {
                continue;
            }
            let rect = neon_glow_rect(base, layer.blur);
            self.fill_rounded_rect(rect, layer.color, radius)?;
        }
        Ok(())
    }

    /// M6c chromatic (`editorial`) — draw an `h1`/`h2` panel-title glyph run with
    /// the RGB-split aberration (`theme-effects.css:34-40`): a red copy at `+dx`
    /// and a cyan copy at `-dx` BEHIND the primary glyph fill, then the normal
    /// title on top. No-op (a plain `draw_text` fall-through) unless the active
    /// effect is `Chromatic`.
    ///
    /// HEADINGS-ONLY (§1.3 / §3.3): route ONLY panel-title draws through this —
    /// never body text, item labels, or pill labels (Tauri scopes it to `h1,h2`).
    /// §10: when `Chromatic`, 3 `draw_text` calls (the existing `utf16_scratch`
    /// is reused, no new alloc); otherwise a single fall-through draw. The
    /// `effect` is passed by value (`Copy`).
    fn draw_text_chromatic_title(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        if let bento_nano_style::tokens::EffectTauri::Chromatic(c) = effect {
            let (red_x, cyan_x) = chromatic_split_offsets(rect.x, c.dx_dip);
            let red_rect = bento_nano_style::Rect { x: red_x, ..rect };
            let cyan_rect = bento_nano_style::Rect { x: cyan_x, ..rect };
            self.draw_text(text, red_rect, c.red)?;
            self.draw_text(text, cyan_rect, c.cyan)?;
        }
        self.draw_text(text, rect, color)
    }

    /// M1i fidelity (2026-05-29) — stroke a rounded-rect outline (no fill).
    /// Used for the §2 source-card `border: 1px solid var(--border-zen)`. The
    /// stroke is centred on the geometric edge (D2D default), which matches the
    /// CSS `border-box` hairline closely enough at the 1-DIP widths used here.
    fn stroke_rounded_rect(
        &self,
        rect: bento_nano_style::Rect,
        color: Color,
        radius: BorderRadius,
        stroke_width: f32,
    ) -> Result<(), RenderError> {
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 || stroke_width <= 0.0 {
            return Ok(());
        }
        let brush = self.solid_brush(color)?;
        let rr = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            radiusX: radius.top_left,
            radiusY: radius.top_left,
        };
        let ctx = self.ctx()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast.
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr lives for the call; brush COM-ref-counted; the
        // default stroke style (None) is the canonical solid hairline.
        unsafe {
            rt.DrawRoundedRectangle(&rr, &brush, stroke_width, None);
        }
        Ok(())
    }

    /// M6-UI fidelity (2026-05-29) — fill a rectangle rounding ONLY the corners
    /// flagged in `corners` (`[top_left, top_right, bottom_right, bottom_left]`)
    /// to `radius`; flagged-off corners stay square. D2D's `FillRoundedRectangle`
    /// only supports a single uniform radius and there is no rounded-clip
    /// primitive (`PushAxisAlignedClip` is rectangular), so the per-corner
    /// silhouette is materialised as a closed `ID2D1PathGeometry` (one
    /// arc per rounded corner, straight `AddLine` for square ones). This is the
    /// visible-correct approximation for Tauri's `.theme-card__swatches
    /// { border-radius: 8px; overflow: hidden }` masking the 2×2 quadrants:
    /// each corner quadrant rounds only its single OUTER corner so the four
    /// quadrants meet square at the centre cross while the block silhouette is
    /// an 8-DIP rounded square. Path-sink build uses no Rust String/Vec/format!
    /// (§10) — same mechanism as `svg::build` for icon glyphs.
    fn fill_partial_rounded_rect(
        &self,
        rect: bento_nano_style::Rect,
        color: Color,
        radius: f32,
        corners: [bool; 4],
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D_SIZE_F,
        };
        use windows::Win32::Graphics::Direct2D::{
            D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_SWEEP_DIRECTION_CLOCKWISE,
            ID2D1GeometrySink,
        };
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Clamp the radius so it never exceeds half the shortest edge.
        let r = radius.max(0.0).min(rect.width * 0.5).min(rect.height * 0.5);
        if r <= 0.0 || corners == [false; 4] {
            // Nothing to round — fall back to the cheap square fill.
            return self.fill_rounded_rect(rect, color, BorderRadius::ZERO);
        }
        let l = rect.x;
        let t = rect.y;
        let rt_x = rect.right();
        let b = rect.bottom();
        // Per-corner inset (0 when the corner is square so the figure walks
        // straight into the geometric corner).
        let tl = if corners[0] { r } else { 0.0 };
        let tr = if corners[1] { r } else { 0.0 };
        let br = if corners[2] { r } else { 0.0 };
        let bl = if corners[3] { r } else { 0.0 };
        let arc = |to_x: f32, to_y: f32| D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F { x: to_x, y: to_y },
            size: D2D_SIZE_F { width: r, height: r },
            rotationAngle: 90.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        // Mc-2b: `d2d::factory()` now returns `Arc<D2dFactory>`; bind it to a
        // local so the `&...factory` borrow outlives this statement (a
        // `&...?.factory` temporary Arc would be dropped at the `;`).
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        // SAFETY: factory valid; geometry + sink are freshly created and the
        // sink is closed before this fn returns (mirrors svg::to_d2d_geometry).
        let geom = ok("CreatePathGeometry", unsafe { factory.CreatePathGeometry() })?;
        let sink: ID2D1GeometrySink = ok("PathGeometry::Open", unsafe { geom.Open() })?;
        // Walk the perimeter clockwise from the top edge, arcing rounded
        // corners and cutting straight to the geometric corner on square ones.
        // SAFETY: sink valid until Close() below; all points live on the stack.
        unsafe {
            sink.BeginFigure(
                D2D_POINT_2F { x: l + tl, y: t },
                D2D1_FIGURE_BEGIN_FILLED,
            );
            // Top edge → top-right corner.
            sink.AddLine(D2D_POINT_2F { x: rt_x - tr, y: t });
            if corners[1] {
                sink.AddArc(&arc(rt_x, t + tr));
            }
            // Right edge → bottom-right corner.
            sink.AddLine(D2D_POINT_2F { x: rt_x, y: b - br });
            if corners[2] {
                sink.AddArc(&arc(rt_x - br, b));
            }
            // Bottom edge → bottom-left corner.
            sink.AddLine(D2D_POINT_2F { x: l + bl, y: b });
            if corners[3] {
                sink.AddArc(&arc(l, b - bl));
            }
            // Left edge → top-left corner.
            sink.AddLine(D2D_POINT_2F { x: l, y: t + tl });
            if corners[0] {
                sink.AddArc(&arc(l + tl, t));
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
        // SAFETY: sink valid; Close finalises the geometry before any fill.
        ok("GeometrySink::Close", unsafe { sink.Close() })?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; geom + brush outlive the call; no transform change.
        unsafe {
            ctx.FillGeometry(&geom, &brush, None);
        }
        Ok(())
    }

    fn draw_text(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        let format = self.text_format.clone();
        self.draw_text_with_format(text, rect, color, &format)
    }

    /// RC-4 Gap 3 — single-line variant of `draw_text` that disables DWrite
    /// word-wrap and character-trims with an ellipsis when the glyph run
    /// exceeds `rect.width`. Used by BulkManager action buttons whose
    /// 4-letter Latin labels ("Show", "Move", "Close") were wrapping into
    /// "Sho/w", "Mov", "Clos/e" against the wider YaHei UI fallback metrics.
    fn draw_text_no_wrap(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format.clone();
        // RC-5 Gap A — lazy-create the `…` trimming sign on first paint after
        // a format recreate. Without a sign, `SetTrimming(_, None)` silently
        // drops trailing glyphs and users can't tell the label was clipped.
        if self.ellipsis_sign.is_none() {
            self.ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            self.ellipsis_sign.as_ref(),
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    fn draw_text_with_style(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
    ) -> Result<(), RenderError> {
        let format = self.text_format_for_style(size_pt, weight, line_height)?;
        self.draw_text_with_format(text, rect, color, &format)
    }

    fn draw_text_with_format(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        format: &IDWriteTextFormat,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Reuse the UTF-16 scratch buffer (spec §10 hot-path).
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout(
            &self.utf16_scratch,
            format,
            rect.width.max(1.0),
            rect.height.max(1.0),
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    fn text_format_for_style(
        &mut self,
        size_pt: f32,
        weight: u16,
        line_height: f32,
    ) -> Result<IDWriteTextFormat, RenderError> {
        let size_pt = size_pt.max(1.0);
        let weight = dwrite::normalize_font_weight(weight);
        let line_height = dwrite::normalize_line_height(line_height);
        if (self.text_format_size_pt - size_pt).abs() < f32::EPSILON
            && self.text_format_weight == weight
            && (self.text_format_line_height - line_height).abs() < f32::EPSILON
        {
            return Ok(self.text_format.clone());
        }
        for cached in &self.text_format_cache {
            if cached.family == self.text_format_family
                && (cached.size_pt - size_pt).abs() < f32::EPSILON
                && cached.weight == weight
                && (cached.line_height - line_height).abs() < f32::EPSILON
            {
                return Ok(cached.format.clone());
            }
        }
        let family = self.text_format_family.clone();
        let format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            weight,
            line_height,
            dwrite::locale_zh_cn(),
        )?;
        let entry = CachedTextFormat {
            family,
            size_pt,
            weight,
            line_height,
            format: format.clone(),
        };
        if self.text_format_cache.len() >= TEXT_FORMAT_CACHE_CAPACITY {
            self.text_format_cache[0] = entry;
        } else {
            self.text_format_cache.push(entry);
        }
        Ok(format)
    }

    /// M1i fidelity (2026-05-29) — lazily create/cache the monospace text
    /// format for the §2 source-card path line. Tauri's `.desktop-source-card
    /// __path` uses `font-family: ui-monospace, Consolas, monospace`; Consolas
    /// is the Win10/11 fixed-pitch system font (no bundled `.ttf`, spec §5).
    /// `size_pt` is the path font size in DIP (11). Cached against the size so
    /// a theme swap (which only touches the proportional body font) never
    /// invalidates it. One COM allocation per recreate, zero per frame.
    fn ensure_monospace_format(
        &mut self,
        size_pt: f32,
    ) -> Result<IDWriteTextFormat, RenderError> {
        let size_pt = size_pt.max(1.0);
        if let Some(cached) = self.monospace_format.as_ref() {
            if (cached.size_pt - size_pt).abs() < f32::EPSILON {
                return Ok(cached.format.clone());
            }
        }
        // #19-B (2026-05-31) — resolve a MONOSPACE family that DWrite confirms
        // is installed BEFORE creating the format, so a stripped SKU lacking
        // Consolas never falls through `text_format_from_family_name`'s
        // proportional fallback into a wrong-metric body face. Normal Windows
        // has Consolas → identical to before (Q2 pixel-1:1).
        let family = SmolStr::new_static(dwrite::resolve_default_family(
            dwrite::FontRole::Monospace,
            &[
                "Consolas",
                "Cascadia Mono",
                "Cascadia Code",
                "Lucida Console",
                "Courier New",
            ],
        ));
        let format = dwrite::text_format_from_family_name_with_metrics(
            family.as_str(),
            size_pt,
            400,
            1.2,
            dwrite::locale_zh_cn(),
        )?;
        self.monospace_format = Some(CachedTextFormat {
            family,
            size_pt,
            weight: 400,
            line_height: 1.2,
            format: format.clone(),
        });
        // A new monospace format invalidates the monospace `…` sign.
        self.monospace_ellipsis_sign = None;
        Ok(format)
    }

    /// M1i fidelity — draw the §2 source-card path line in the monospace format
    /// with DWrite character-trimming (`…`) when it overflows `rect.width`.
    /// Mirrors Tauri's `overflow: hidden; text-overflow: ellipsis; white-space:
    /// nowrap` on `.desktop-source-card__path`.
    fn draw_text_monospace_ellipsis(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.ensure_monospace_format(size_pt)?;
        if self.monospace_ellipsis_sign.is_none() {
            self.monospace_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            self.monospace_ellipsis_sign.as_ref(),
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    /// M1i fidelity — draw a short single-line label centred horizontally AND
    /// vertically inside `rect`. Used for the 28-DIP icon-circle initial glyph,
    /// the `↻` refresh glyph, and the watched badge text so they read as
    /// optically centred chips (Tauri uses `display:flex; align-items:center;
    /// justify-content:center`). Reuses the active body format; no wrap.
    fn draw_text_centered(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
        };
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(size_pt, weight, 1.0)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
        )?;
        // SAFETY: freshly-created layout; both Set* calls only mutate per-layout
        // alignment state and take canonical enum values.
        unsafe {
            let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        }
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    /// M6-UI fidelity (2026-05-29) — draw an UPPERCASE, letter-tracked label.
    /// Mirrors Tauri `.theme-group__title { text-transform: uppercase;
    /// letter-spacing: 1px }`. The `text` is upper-cased the same way the
    /// watched badge path does (`to_uppercase()` — a no-op for the CJK zh
    /// headings 圆角玻璃/实心/方角现代/个性, an EN-glyph caps fold otherwise),
    /// and the 1-DIP per-glyph tracking is applied via DWrite
    /// `IDWriteTextLayout1::SetCharacterSpacing` (trailing advance) over the
    /// whole run — the true typographic equivalent of CSS letter-spacing, for
    /// both locales. The `to_uppercase()` allocation matches the already-shipped
    /// badge pattern (§10: the headings paint once per visible frame, not on the
    /// per-item hot path).
    fn draw_text_tracked(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        tracking: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{IDWriteTextLayout1, DWRITE_TEXT_RANGE};
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let upper = text.to_uppercase();
        let format = self.text_format_for_style(size_pt, weight, 1.0)?;
        self.utf16_scratch.clear();
        for u in upper.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
        )?;
        // SetCharacterSpacing lives on IDWriteTextLayout1 — cross-cast per
        // spec §15.1 (canonical Interface::cast). Apply `tracking` as the
        // trailing advance over the entire glyph run; leading + min-advance 0.
        let layout1: IDWriteTextLayout1 =
            ok("TextLayout::cast<TextLayout1>", layout.cast())?;
        let range = DWRITE_TEXT_RANGE {
            startPosition: 0,
            length: self.utf16_scratch.len() as u32,
        };
        // SAFETY: layout1 is a freshly-created COM interface; SetCharacterSpacing
        // only mutates per-instance spacing state over the canonical full range.
        unsafe {
            let _ = layout1.SetCharacterSpacing(0.0, tracking, 0.0, range);
        }
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout1, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
        Ok(())
    }

    /// Draw a 1:1 SVG path translated into `rect.origin`. Caller takes
    /// responsibility for sizing — `draw_svg_fit` is the safer entry when
    /// the path's viewbox doesn't match the destination rect.
    fn draw_svg(
        &self,
        path_d: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = svg::build(factory, path_d)?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        // 1:1 translate — the path is already the right size.
        // Phase 2.3.1b — compose against `base_scale` so high-DPI frames
        // still project the glyph onto the correct device pixel grid.
        // `base_scale * translate(rect.x, rect.y) * identity-glyph-scale`.
        let s = self.base_scale;
        let m = windows::Foundation::Numerics::Matrix3x2 {
            M11: s,
            M12: 0.0,
            M21: 0.0,
            M22: s,
            M31: rect.x * s,
            M32: rect.y * s,
        };
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the per-frame base scale so subsequent draw calls
            // (e.g., the next SVG glyph or fill_rounded_rect) keep using
            // logical coords without extra bookkeeping.
            let base = base_scale_matrix(s);
            ctx.SetTransform(&base);
        }
        Ok(())
    }

    /// Draw an SVG path scaled-to-fit inside `rect`. `view_size` is the
    /// edge length of the source viewbox (typical Lucide / Material glyphs
    /// are 24). Uniform scale preserves the icon's aspect ratio; the glyph
    /// is centred on whichever axis has spare room.
    fn draw_svg_fit(
        &self,
        path_d: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        view_size: f32,
    ) -> Result<(), RenderError> {
        if view_size <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = svg::build(factory, path_d)?;
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        let scale = (rect.width / view_size).min(rect.height / view_size);
        let glyph_w = view_size * scale;
        let glyph_h = view_size * scale;
        let dx = rect.x + (rect.width - glyph_w) * 0.5;
        let dy = rect.y + (rect.height - glyph_h) * 0.5;
        // Phase 2.3.1b — compose `base_scale * (glyph_scale, translate(dx, dy))`.
        // `M11 = base_scale * glyph_scale` collapses both projections into a
        // single 3×2 multiply on the stack — no extra `Matrix3x2::multiply`
        // call (§10 hot-path: float ops in registers, no alloc).
        let bs = self.base_scale;
        let combined = scale * bs;
        let m = windows::Foundation::Numerics::Matrix3x2 {
            M11: combined,
            M12: 0.0,
            M21: 0.0,
            M22: combined,
            M31: dx * bs,
            M32: dy * bs,
        };
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the per-frame base scale so subsequent draw calls
            // keep using logical coords.
            let base = base_scale_matrix(bs);
            ctx.SetTransform(&base);
        }
        Ok(())
    }

    /// RC-4 Gap 1 — render a zone-icon name as a real line-art glyph.
    ///
    /// `name` is the wire-format icon string from `Zone.icon` (e.g. "folder",
    /// "settings", "search"). When it resolves to a built-in `IconKind`, the
    /// matching 24×24 source SVG document is drawn via
    /// `draw_svg_document_stroke_fit` (cached geometry). When it doesn't,
    /// we fall back to the legacy text path so unknown / emoji / lucide
    /// names keep rendering as a single text run.
    fn draw_icon_glyph(
        &mut self,
        name: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if let Some(kind) = IconKind::from_str_opt(name) {
            // 24-unit viewbox per `IconKind::source_svg` — every built-in is
            // hand-rolled around 0–24 just like the 1.x Tauri sources.
            return self.draw_svg_document_stroke_fit(kind.source_svg(), rect, color, 24.0);
        }
        self.draw_text(name, rect, color)
    }

    fn draw_svg_document_stroke_fit(
        &mut self,
        svg_document: &'static str,
        rect: bento_nano_style::Rect,
        color: Color,
        view_size: f32,
    ) -> Result<(), RenderError> {
        if view_size <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        // Mc-2b: bind the `Arc<D2dFactory>` to a local before borrowing `.factory`.
        let d2d_fac = d2d::factory()?;
        let factory = &d2d_fac.factory;
        let geom = {
            let cached = self
                .svg_cache
                .get_or_insert(svg_document.as_bytes(), factory)?;
            cached.clone()
        };
        let brush = self.solid_brush(color)?;
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        let scale = (rect.width / view_size).min(rect.height / view_size);
        let glyph_w = view_size * scale;
        let glyph_h = view_size * scale;
        let dx = rect.x + (rect.width - glyph_w) * 0.5;
        let dy = rect.y + (rect.height - glyph_h) * 0.5;
        let bs = self.base_scale;
        let combined = scale * bs;
        let m = windows::Foundation::Numerics::Matrix3x2 {
            M11: combined,
            M12: 0.0,
            M21: 0.0,
            M22: combined,
            M31: dx * bs,
            M32: dy * bs,
        };
        // SAFETY: rt valid; geometry and brush are COM references alive for
        // the call; matrix lives on the stack; `None` uses D2D's default
        // round-cap/round-join behavior encoded by the source line art.
        unsafe {
            rt.SetTransform(&m);
            rt.DrawGeometry(&geom, &brush, 1.5, None);
            let base = base_scale_matrix(bs);
            rt.SetTransform(&base);
        }
        Ok(())
    }

    fn solid_brush(&self, c: Color) -> Result<ID2D1SolidColorBrush, RenderError> {
        Ok(d2d::solid_brush(self.ctx()?, c.r, c.g, c.b, c.a)?)
    }
}

// M6-UI (2026-05-29) — the Wave J1b `ThemePickerAdapter` (the
// `RendererLike` bridge that forwarded the popup `paint_into` onto the
// renderer) was removed alongside the popup. §3 Appearance now paints inline
// in `draw_settings_panel`'s body closure using the renderer's own
// `fill_rounded_rect` / `stroke_rounded_rect` / `draw_text` directly, so no
// adapter trait object is needed.

/// Phase 2.3.1b — pure-scale 3×2 matrix used as the per-frame base
/// transform. Free function so caller sites avoid an extra `&self` borrow
/// when they only need the matrix value (e.g., between back-to-back SVG
/// transform restores).
#[inline]
fn base_scale_matrix(scale: f32) -> windows::Foundation::Numerics::Matrix3x2 {
    windows::Foundation::Numerics::Matrix3x2 {
        M11: scale,
        M12: 0.0,
        M21: 0.0,
        M22: scale,
        M31: 0.0,
        M32: 0.0,
    }
}

/// Mc-2b — pure staleness predicate for the paint-entry generation self-heal.
/// Returns `true` when the renderer's cached device generation no longer
/// matches the platform's current generation, i.e. the device chain was
/// rebuilt (by this or another window's recovery) since this renderer last
/// built its device-derived COM. Free function so the decision is unit-testable
/// without a GPU-backed `Renderer`.
#[inline]
fn renderer_is_stale(cached_gen: u64, current_gen: u64) -> bool {
    cached_gen != current_gen
}

fn active_item_drag_visual(app: &AppState) -> Option<ActiveItemDragVisual> {
    let drag = app.item_drag.borrow();
    let candidate = drag.as_ref()?;
    if !candidate.is_internal_dragging {
        return None;
    }
    Some(ActiveItemDragVisual {
        zone_id: candidate.zone_id,
        item_id: candidate.item_id,
        last_x: candidate.last_x as f32,
        last_y: candidate.last_y as f32,
    })
}

fn hit_test_render_zone(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    for zone in app.zones.iter().rev() {
        if !zone.is_visible() || zone.is_stacked_child() {
            continue;
        }
        let left = zone.x as f32;
        let top = zone.y as f32;
        let right = left + zone.w as f32;
        let bottom = top + zone.h as f32;
        if x >= left && x < right && y >= top && y < bottom {
            return Some(zone.id);
        }
    }
    None
}

fn drop_preview_rect_for_zone(
    zone: &Zone,
    drag: Option<ActiveItemDragVisual>,
    is_wide: bool,
) -> Option<bento_nano_style::Rect> {
    let drag = drag?;
    let (grid_x, grid_y) = item_grid_position_for_zone(zone, drag.last_x, drag.last_y);
    let rect = item_card_rect_for_grid(zone, grid_x, grid_y, is_wide);
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

fn item_grid_position_for_zone(zone: &Zone, x: f32, y: f32) -> (i32, i32) {
    let columns = zone.grid_columns.max(1) as i32;
    let columns_f = columns as f32;
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    let cell_w = ((zone.w as f32 - 16.0) - gap * (columns_f - 1.0)).max(44.0) / columns_f;
    let col_stride = cell_w + gap;
    let row_stride = item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX;
    let raw_col = ((x - zone.x as f32 - 8.0) / col_stride).floor() as i32;
    let raw_row = ((y - zone.y as f32 - 30.0) / row_stride).floor() as i32;
    (raw_col.clamp(0, columns - 1), raw_row.max(0))
}

fn item_card_rect_for_grid(
    zone: &Zone,
    grid_x: i32,
    grid_y: i32,
    is_wide: bool,
) -> bento_nano_style::Rect {
    highlight_overlay::item_card_rect_for_grid(zone, grid_x, grid_y, is_wide)
}

fn source_drag_item(app: &AppState, drag: ActiveItemDragVisual) -> Option<(&Zone, &ZoneItem)> {
    let zone = app.zones.get(drag.zone_id)?;
    let item = zone.item(drag.item_id)?;
    Some((zone, item))
}

fn drag_ghost_rect(
    app: &AppState,
    drag: ActiveItemDragVisual,
    source_rect: bento_nano_style::Rect,
) -> bento_nano_style::Rect {
    let width = source_rect.width.max(64.0);
    let height = source_rect.height.max(48.0);
    let max_x = (app.viewport.width - width).max(0.0);
    let max_y = (app.viewport.height - height).max(0.0);
    bento_nano_style::Rect {
        x: (drag.last_x - width * 0.5).clamp(0.0, max_x),
        y: (drag.last_y - 18.0).clamp(0.0, max_y),
        width,
        height,
    }
}

fn inset_rect(rect: bento_nano_style::Rect, inset: f32) -> bento_nano_style::Rect {
    bento_nano_style::Rect {
        x: rect.x + inset,
        y: rect.y + inset,
        width: (rect.width - inset * 2.0).max(0.0),
        height: (rect.height - inset * 2.0).max(0.0),
    }
}

// =============================================================================
// M6c — pure effect geometry (testable, no GPU). The 3 render primitives
// (`draw_scanline_overlay` / `draw_neon_glow` / `draw_text_chromatic_title`)
// delegate their math here so it can be unit-tested without a live D2D target
// (§3.4: no offscreen render harness exists). Every helper is allocation-free
// stack-`f32` math (§10) and panic-free (§11).
// =============================================================================

/// M6c scanline — the number of 1-DIP-tall lit bands a full-viewport overlay
/// of height `vh` paints at period `period`. Bands sit at `y = k * period` for
/// `k = 0..count`, so `count = ceil(vh / period)`. A non-positive period or
/// height yields 0 (the overlay no-ops). Pure (§10), panic-free (§11).
fn scanline_band_count(vh: f32, period: f32) -> usize {
    if vh <= 0.0 || period <= 0.0 {
        return 0;
    }
    (vh / period).ceil() as usize
}

/// M6c neon — grow a base rect by `blur` on all four sides (the `drop-shadow(0
/// 0 Npx)` symmetric bloom: 0,0 offset, grown by the blur radius). Mirrors the
/// `draw_shadow_stack` grow-and-fill idiom. Pure (§10).
fn neon_glow_rect(base: bento_nano_style::Rect, blur: f32) -> bento_nano_style::Rect {
    let grow = blur.max(0.0);
    bento_nano_style::Rect {
        x: base.x - grow,
        y: base.y - grow,
        width: base.width + grow * 2.0,
        height: base.height + grow * 2.0,
    }
}

/// M6c chromatic — the two channel-copy x-origins for an `h1`/`h2` glyph run:
/// red at `base_x + dx`, cyan at `base_x - dx` (Tauri `text-shadow 1px 0` /
/// `-1px 0`). Returns `(red_x, cyan_x)`. Pure (§10).
fn chromatic_split_offsets(base_x: f32, dx: f32) -> (f32, f32) {
    (base_x + dx, base_x - dx)
}

/// M6c neon (morph path) — lerp one neon glow `Shadow` layer from its collapsed
/// endpoint `a` to its expanded endpoint `b` by `t` (clamped 0..=1). Blur and
/// every colour channel interpolate so the capsule<->panel morph grows the
/// bloom smoothly with no pop at either endpoint. Pure (§10).
fn lerp_neon_layer(
    a: bento_nano_style::Shadow,
    b: bento_nano_style::Shadow,
    t: f32,
) -> bento_nano_style::Shadow {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    bento_nano_style::Shadow::drop(
        0.0,
        0.0,
        lerp(a.blur, b.blur),
        Color {
            r: lerp(a.color.r, b.color.r),
            g: lerp(a.color.g, b.color.g),
            b: lerp(a.color.b, b.color.b),
            a: lerp(a.color.a, b.color.a),
        },
    )
}

fn timeline_detail_thumbnail_rect(
    panel: bento_nano_style::Rect,
    detail_x: f32,
    detail_w: f32,
) -> bento_nano_style::Rect {
    let y = panel.y + timeline_panel::RUNTIME_ROW_TOP_PX + 86.0;
    let max_h = (panel.bottom() - y - 18.0).max(64.0);
    let max_w = detail_w.clamp(0.0, timeline_panel::THUMBNAIL_MAX_WIDTH);
    let mut width = max_w;
    let mut height = (width / timeline_panel::THUMBNAIL_ASPECT_RATIO).min(max_h);
    if height * timeline_panel::THUMBNAIL_ASPECT_RATIO < width {
        width = height * timeline_panel::THUMBNAIL_ASPECT_RATIO;
    }
    if width < 1.0 || height < 1.0 {
        width = 0.0;
        height = 0.0;
    }
    bento_nano_style::Rect {
        x: detail_x,
        y,
        width,
        height,
    }
}

fn snapshot_row_preview_rect(row: bento_nano_style::Rect) -> bento_nano_style::Rect {
    let height = (row.height - 8.0).max(0.0);
    let width = (height * timeline_panel::THUMBNAIL_ASPECT_RATIO).min(76.0);
    bento_nano_style::Rect {
        x: (row.right() - width - 8.0).max(row.x + 8.0),
        y: row.y + 4.0,
        width,
        height,
    }
}

fn snapshot_zone_thumbnail_rect(
    zone: &SnapshotZone,
    thumbnail: bento_nano_style::Rect,
) -> Option<bento_nano_style::Rect> {
    if !zone.visible {
        return None;
    }
    let canvas = inset_rect(thumbnail, 8.0);
    if canvas.width <= 0.0 || canvas.height <= 0.0 {
        return None;
    }
    let x = canvas.x + canvas.width * percent_ratio(zone.position.x_percent);
    let y = canvas.y + canvas.height * percent_ratio(zone.position.y_percent);
    let right_limit = canvas.right();
    let bottom_limit = canvas.bottom();
    if x >= right_limit || y >= bottom_limit {
        return None;
    }
    let width = (canvas.width * percent_ratio(zone.expanded_size.w_percent))
        .max(3.0)
        .min(right_limit - x);
    let height = (canvas.height * percent_ratio(zone.expanded_size.h_percent))
        .max(3.0)
        .min(bottom_limit - y);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(bento_nano_style::Rect {
        x,
        y,
        width,
        height,
    })
}

fn percent_ratio(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 100.0) as f32 * 0.01
    } else {
        0.0
    }
}

fn grid_columns_label(columns: u32) -> &'static str {
    match columns {
        2 => "2 columns",
        3 => "3 columns",
        4 => "4 columns",
        5 => "5 columns",
        6 => "6 columns",
        _ => "4 columns",
    }
}

/// Wave C — format a zone item count for the collapsed pill badge. Caps
/// the display at "99+" so the badge geometry doesn't need to grow past
/// `PILL_BADGE_MIN_WIDTH` for typical zones; >999 items is still rendered
/// as "999+" so the result fits the 4-digit budget in
/// `zone_pill_geometry::badge_width_for_count`.
fn format_small_count(count: usize) -> smol_str::SmolStr {
    // <1000 renders the literal count; >=1000 caps at the 4-char "999+"
    // budget (the <100 vs <1000 split produced identical text, so merged).
    if count < 1000 {
        smol_str::SmolStr::new(count.to_string())
    } else {
        smol_str::SmolStr::new_static("999+")
    }
}

fn live_folder_badge_text(path: &str) -> smol_str::SmolStr {
    const MAX_PATH_CHARS: usize = 96;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return smol_str::SmolStr::new_static("Live folder: <invalid path>");
    }
    let char_count = trimmed.chars().count();
    if char_count <= MAX_PATH_CHARS {
        return smol_str::SmolStr::new(format!("Live: {trimmed}"));
    }
    let head: String = trimmed.chars().take(44).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(44)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    smol_str::SmolStr::new(format!("Live: {head}…{tail}"))
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// G3 — locale-aware mapping for `SettingsEncryptionMode::as_wire()`. The wire
/// variant returns the SmolStr/SerDe token; this returns the user-visible
/// translation while preserving the same set of distinct states.
//
// β carry-over (Wave I-α / R14 2026-05-25): function landed in the Wave H baseline
// (commit 1562751, 2026-05-20) ahead of the encryption settings UI integration
// and has no current call site. Annotated `#[allow(dead_code)]` so clippy
// `dead_code` lint passes; deletion deferred to β1 when the encryption status
// row is wired up.
#[allow(dead_code)]
fn localized_encryption_mode(mode: crate::state::SettingsEncryptionMode) -> &'static str {
    use crate::state::SettingsEncryptionMode;
    match mode {
        SettingsEncryptionMode::None => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_NONE)
        }
        SettingsEncryptionMode::Dpapi => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_DPAPI)
        }
        SettingsEncryptionMode::Passphrase => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_PASSPHRASE)
        }
    }
}

/// G3 — locale-aware version of `SettingsUpdaterStatus::summary()`. The static
/// `Idle` / `Checking` tokens are translated; the version-bearing variants
/// keep the existing `format!` shape (with a localized prefix) so the
/// "Available 2.1.0" / "Downloading 4096/8192 B" inline-test expectations in
/// `bento-nano-shell` still hold for the en-US locale.
//
// β carry-over (Wave I-α / R14 2026-05-25): Wave H baseline leftover (see
// `localized_encryption_mode` note above). Updater summary row not yet wired
// into the Settings panel; β1 owner of updater UI will either delete or call.
#[allow(dead_code)]
fn localized_updater_summary(status: &crate::state::SettingsUpdaterStatus) -> smol_str::SmolStr {
    use crate::state::SettingsUpdaterStatus;
    match status {
        SettingsUpdaterStatus::Idle => smol_str::SmolStr::new(bento_nano_style::t(
            bento_nano_style::i18n_zh_cn::ids::UPDATER_IDLE,
        )),
        SettingsUpdaterStatus::Checking => smol_str::SmolStr::new(bento_nano_style::t(
            bento_nano_style::i18n_zh_cn::ids::UPDATER_CHECKING,
        )),
        // Version-bearing variants fall through to the wire summary so we
        // don't fork the format strings (those carry SemVer / byte counts
        // that downstream tests rely on verbatim).
        _ => status.summary(),
    }
}

/// G3 — locale-aware version of `SettingsUpdaterStatus::action_label()`.
//
// β carry-over (Wave I-α / R14 2026-05-25): pairs with `localized_updater_summary`
// above; activated by β1 updater UI wave or removed alongside.
#[allow(dead_code)]
fn localized_updater_action_label(status: &crate::state::SettingsUpdaterStatus) -> &'static str {
    use crate::state::SettingsUpdaterStatus;
    match status {
        SettingsUpdaterStatus::Available { .. } => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_DOWNLOAD)
        }
        SettingsUpdaterStatus::Ready { .. } => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_INSTALL)
        }
        SettingsUpdaterStatus::Installing { .. } | SettingsUpdaterStatus::Downloading { .. } => {
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_WAIT)
        }
        _ => bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_DOWNLOAD),
    }
}

fn parse_hex_color(raw: &str) -> Option<Color> {
    let bytes = raw.as_bytes();
    if bytes.len() != 7 || bytes.first().copied() != Some(b'#') {
        return None;
    }
    let r = parse_hex_byte(bytes[1], bytes[2])?;
    let g = parse_hex_byte(bytes[3], bytes[4])?;
    let b = parse_hex_byte(bytes[5], bytes[6])?;
    Some(Color::from_u8(r, g, b, 0xE0))
}

fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn wizard_step_label(step: WizardStep) -> &'static str {
    match step {
        WizardStep::Conditions => "Conditions",
        WizardStep::Action => "Action",
        WizardStep::Preview => "Preview",
        WizardStep::Name => "Name",
        WizardStep::Review => "Review",
    }
}

fn combine_label(mode: rules_wizard::CombineMode) -> &'static str {
    match mode {
        rules_wizard::CombineMode::All => "all",
        rules_wizard::CombineMode::Any => "any",
    }
}

fn predicate_label(kind: PredicateKind) -> &'static str {
    match kind {
        PredicateKind::NameStartsWith => "name starts with",
        PredicateKind::NameContains => "name contains",
        PredicateKind::NameEndsWith => "name ends with",
        PredicateKind::ExtensionIn => "extension in",
        PredicateKind::CreatedBefore => "created before days",
        PredicateKind::ModifiedBefore => "modified before days",
        PredicateKind::SizeGreaterThan => "size greater than",
        PredicateKind::InZone => "in zone",
        PredicateKind::OnDesktop => "on desktop",
    }
}

fn action_label(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::MoveToZone => "move to zone",
        ActionKind::MoveToFolder => "move to folder",
        ActionKind::DeleteToRecycleBin => "delete to recycle bin",
        ActionKind::Tag => "tag",
        ActionKind::Notify => "notify",
    }
}

fn run_mode_label(mode: RunModeChoice) -> &'static str {
    match mode {
        RunModeChoice::OnDemand => "on demand",
        RunModeChoice::OnFileChange => "on file change",
        RunModeChoice::Interval => "interval",
    }
}

#[cfg(test)]
mod device_loss_tests {
    use super::renderer_is_stale;

    #[test]
    fn same_generation_is_not_stale() {
        assert!(!renderer_is_stale(0, 0));
        assert!(!renderer_is_stale(7, 7));
        assert!(!renderer_is_stale(u64::MAX, u64::MAX));
    }

    #[test]
    fn changed_generation_is_stale() {
        // Generation only ever increases (one bump per recover_device_chain),
        // but the predicate is a plain inequality so direction is irrelevant.
        assert!(renderer_is_stale(0, 1));
        assert!(renderer_is_stale(3, 4));
        assert!(renderer_is_stale(1, 0));
    }
}

#[cfg(test)]
mod m6c_effect_geometry_tests {
    use super::{chromatic_split_offsets, lerp_neon_layer, neon_glow_rect, scanline_band_count};
    use bento_nano_style::{Color, Shadow};

    #[test]
    fn scanline_band_count_ceils_height_over_period() {
        // vp height 100, period 3 → ceil(100/3) = 34 bands (y = 0,3,...,99).
        assert_eq!(scanline_band_count(100.0, 3.0), 34);
        // Exact multiple: height 99, period 3 → 33 bands (y = 0..96, last < 99).
        assert_eq!(scanline_band_count(99.0, 3.0), 33);
        // A tall 1080 surface at period 3 → 360 bands.
        assert_eq!(scanline_band_count(1080.0, 3.0), 360);
    }

    #[test]
    fn scanline_band_count_zero_guards() {
        // Non-positive period / height → 0 bands (the overlay no-ops, panic-free).
        assert_eq!(scanline_band_count(0.0, 3.0), 0);
        assert_eq!(scanline_band_count(-5.0, 3.0), 0);
        assert_eq!(scanline_band_count(100.0, 0.0), 0);
        assert_eq!(scanline_band_count(100.0, -1.0), 0);
    }

    #[test]
    fn scanline_loop_steps_match_band_count() {
        // The `draw_scanline_overlay` `while y < height` loop emits exactly
        // `scanline_band_count` fills; mirror its stepping here to pin the count.
        let (height, period) = (100.0_f32, 3.0_f32);
        let mut y = 0.0_f32;
        let mut n = 0usize;
        while y < height {
            n += 1;
            y += period;
        }
        assert_eq!(n, scanline_band_count(height, period));
    }

    #[test]
    fn neon_glow_rect_grows_all_sides_by_blur() {
        let base = bento_nano_style::Rect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };
        // blur 6 → grown 6 on every side: {4,4,52,52}.
        let g = neon_glow_rect(base, 6.0);
        assert_eq!(g.x, 4.0);
        assert_eq!(g.y, 4.0);
        assert_eq!(g.width, 52.0);
        assert_eq!(g.height, 52.0);
        // blur 0 → identity (no growth).
        let g0 = neon_glow_rect(base, 0.0);
        assert_eq!(g0, base);
        // negative blur clamps to 0.
        assert_eq!(neon_glow_rect(base, -3.0), base);
    }

    #[test]
    fn neon_draw_order_is_reversed_so_magenta_underlies_cyan() {
        // The authored array is `[cyan_inner, magenta_outer]`; `draw_neon_glow`
        // iterates `.iter().rev()` so the wider magenta (index 1) paints first
        // and the tighter cyan (index 0) sits on top. Pin that order here.
        let cyan = Shadow::drop(0.0, 0.0, 6.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
        let magenta = Shadow::drop(0.0, 0.0, 12.0, Color::from_u8(0xFF, 0x2E, 0x93, 0x66));
        let layers = [cyan, magenta];
        let drawn: Vec<f32> = layers.iter().rev().map(|l| l.blur).collect();
        // Wider magenta (12) drawn first, tighter cyan (6) drawn last (on top).
        assert_eq!(drawn, vec![12.0, 6.0]);
    }

    #[test]
    fn chromatic_offsets_split_red_right_cyan_left() {
        // base_x 50, dx 1 → red at 51 (+dx), cyan at 49 (-dx).
        let (red_x, cyan_x) = chromatic_split_offsets(50.0, 1.0);
        assert_eq!(red_x, 51.0);
        assert_eq!(cyan_x, 49.0);
    }

    #[test]
    fn lerp_neon_layer_endpoints_and_midpoint() {
        let a = Shadow::drop(0.0, 0.0, 6.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
        let b = Shadow::drop(0.0, 0.0, 8.0, Color::from_u8(0x00, 0xF0, 0xFF, 0xFF));
        // t=0 → collapsed blur 6.
        assert_eq!(lerp_neon_layer(a, b, 0.0).blur, 6.0);
        // t=1 → expanded blur 8.
        assert_eq!(lerp_neon_layer(a, b, 1.0).blur, 8.0);
        // t=0.5 → midpoint blur 7.
        assert_eq!(lerp_neon_layer(a, b, 0.5).blur, 7.0);
        // Out-of-range t clamps (easeOutBack overshoot never over-grows).
        assert_eq!(lerp_neon_layer(a, b, 1.5).blur, 8.0);
        assert_eq!(lerp_neon_layer(a, b, -0.2).blur, 6.0);
    }
}

#[cfg(test)]
mod item_drag_visual_tests {
    use super::*;
    use std::borrow::Cow;

    fn snapshot_zone(
        visible: bool,
        x_percent: f64,
        y_percent: f64,
        w_percent: f64,
        h_percent: f64,
    ) -> SnapshotZone {
        SnapshotZone {
            id: smol_str::SmolStr::new_static("z1"),
            name: "Zone".to_owned(),
            icon: smol_str::SmolStr::new_static("folder"),
            position: bento_nano_backend::layout::RelativePosition {
                x_percent,
                y_percent,
            },
            expanded_size: bento_nano_backend::layout::RelativeSize {
                w_percent,
                h_percent,
            },
            items: Vec::new(),
            accent_color: Some(smol_str::SmolStr::new_static("#3b82f6")),
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: smol_str::SmolStr::new_static(""),
            updated_at: smol_str::SmolStr::new_static(""),
            capsule_size: smol_str::SmolStr::new_static("medium"),
            capsule_shape: smol_str::SmolStr::new_static("pill"),
            locked: false,
            visible,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    #[test]
    fn drop_preview_uses_renderer_grid_geometry() {
        let zone = Zone::new(ZoneId(7), Cow::Borrowed("z"), 10, 20, 240, 180);
        let drag = ActiveItemDragVisual {
            zone_id: ZoneId(1),
            item_id: ZoneItemId(1),
            last_x: 130.0,
            last_y: 116.0,
        };

        let rect = drop_preview_rect_for_zone(&zone, Some(drag), false).expect("preview");

        assert!(rect.x >= 10.0);
        assert!(rect.y >= 20.0);
        assert!(rect.right() <= 250.0);
        assert!(rect.bottom() <= 200.0);
    }

    #[test]
    fn live_folder_badge_text_preserves_visible_path_and_compacts_long_paths() {
        let short = live_folder_badge_text("C:/Users/HP/Documents/Live");
        assert_eq!(short.as_str(), "Live: C:/Users/HP/Documents/Live");

        let long = live_folder_badge_text(
            "C:/Users/HP/Documents/VeryLongLiveFolderPath/with/many/segments/that/should/still/show/both/prefix/and/suffix",
        );
        assert!(long.as_str().starts_with("Live: C:/Users/HP/"));
        assert!(long.as_str().contains('…'));
        assert!(long.as_str().ends_with("show/both/prefix/and/suffix"));
    }

    #[test]
    fn drag_ghost_is_clamped_to_viewport() {
        let mut app = AppState::new();
        app.viewport = bento_nano_style::Size {
            width: 120.0,
            height: 96.0,
        };
        let drag = ActiveItemDragVisual {
            zone_id: ZoneId(1),
            item_id: ZoneItemId(1),
            last_x: 400.0,
            last_y: 400.0,
        };
        let source = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 64.0,
        };

        let ghost = drag_ghost_rect(&app, drag, source);

        assert_eq!(ghost.x, 40.0);
        assert_eq!(ghost.y, 32.0);
    }

    #[test]
    fn snapshot_thumbnail_maps_zone_percentages_into_canvas() {
        let thumbnail = bento_nano_style::Rect {
            x: 10.0,
            y: 20.0,
            width: 160.0,
            height: 96.0,
        };
        let zone = snapshot_zone(true, 50.0, 25.0, 25.0, 50.0);

        let rect = snapshot_zone_thumbnail_rect(&zone, thumbnail).expect("visible zone");

        assert!((rect.x - 90.0).abs() < 0.01);
        assert!((rect.y - 48.0).abs() < 0.01);
        assert!((rect.width - 36.0).abs() < 0.01);
        assert!((rect.height - 40.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_thumbnail_skips_hidden_and_out_of_bounds_zones() {
        let thumbnail = bento_nano_style::Rect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 90.0,
        };

        assert!(
            snapshot_zone_thumbnail_rect(&snapshot_zone(false, 0.0, 0.0, 20.0, 20.0), thumbnail)
                .is_none()
        );
        assert!(
            snapshot_zone_thumbnail_rect(&snapshot_zone(true, 100.0, 100.0, 20.0, 20.0), thumbnail)
                .is_none()
        );
    }

    #[test]
    fn snapshot_row_preview_stays_inside_row() {
        let row = bento_nano_style::Rect {
            x: 20.0,
            y: 40.0,
            width: 300.0,
            height: 44.0,
        };

        let rect = snapshot_row_preview_rect(row);

        assert!(rect.x >= row.x);
        assert!(rect.y >= row.y);
        assert!(rect.right() <= row.right());
        assert!(rect.bottom() <= row.bottom());
        assert!((rect.width / rect.height - timeline_panel::THUMBNAIL_ASPECT_RATIO).abs() < 0.01);
    }
}
