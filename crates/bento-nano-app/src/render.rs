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
//! Phase 2 themes let each window kind pick its own system font role while
//! Settings, capsules, and MiniBar can resolve through the shared UI primary.
//! The per-window cost is one COM ref (~1 KB) — well below the 100 MB ceiling
//! even at the §11 R7 max of 8 + 1 windows.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use bento_nano_backend::{
    layout::{BentoZone as SnapshotZone, DesktopSnapshot},
    system::get_memory_usage,
};
use bento_nano_layout::LayoutError;
use bento_nano_platform::{
    Backdrop, PlatformError, WindowKind, backdrop_brush_scale, capture_primary_workarea_blurred,
    d2d::{self, WindowSurface},
    dcomp::WindowComp,
    dwrite, ok, svg,
    svg_cache::SvgCache,
};
use bento_nano_style::{BorderRadius, Color, Lerp, Rect, Shadow, ShadowStack};
use bento_nano_widget::{ImageSource, WidgetNode};
use bento_nano_zone::{Zone, ZoneId, ZoneItem, ZoneItemId};
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
const TEXT_FORMAT_CACHE_CAPACITY: usize = 8;
const IMAGE_WIDGET_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Tauri `.stack-wrapper--bloomed .stack-capsule { transform: scale(0.92) }`.
const STACK_CAPSULE_BLOOMED_SCALE: f32 = 0.92;
/// Tauri `.stack-wrapper--bloomed .stack-capsule { opacity: 0.55 }`.
const STACK_CAPSULE_BLOOMED_OPACITY: f32 = 0.55;
/// Tauri bloomed capsule transition window:
/// `transform/opacity/box-shadow/border-color 180ms`.
const STACK_CAPSULE_BLOOMED_RECEDES_MS: f32 = 180.0;
const STACK_CAPSULE_EMERGE_START_SCALE: f32 = 0.96;
const STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE: f32 = 1.02;
const STACK_CAPSULE_EMERGE_OVERSHOOT_AT: f32 = 0.60;
/// Tauri `.bento-zone--dragging` / `.stack-wrapper--dragging` keep the full
/// capsule geometry and fade the carried surface to 70% opacity.
const ZONE_DRAG_VISUAL_OPACITY: f32 = 0.70;
/// DComp presents the state replacement immediately, whereas Chromium often
/// coalesces the unmount/mount into the first animated frame. Never present a
/// fully transparent native stack replacement: start at three 60 Hz frames of
/// the authored 240 ms keyframe while retaining the same curve and endpoint.
const STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS: f32 =
    48.0 / animator::STACK_EMERGE_DURATION_MS as f32;
/// Tauri Bloom petal hover/active transform and transition.
const STACK_BLOOM_ACTIVE_SCALE: f32 = 1.05;
const STACK_BLOOM_ACTIVE_TRANSITION_MS: u32 = 180;
/// Tauri active-petal halo breathes after a short settle delay.
const STACK_BLOOM_ACTIVE_PULSE_DELAY_MS: u32 = 600;
const STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS: u32 = 1_500;
/// Tauri `.stack-capsule.is-locked { opacity: 0.9 }`.
const STACK_CAPSULE_LOCKED_OPACITY: f32 = 0.9;
/// Tauri `.stack-wrapper--locked .stack-capsule__badge { background: rgba(245, 158, 11, 0.14) }`.
const STACK_CAPSULE_LOCKED_BADGE_FILL: Color = Color::from_u8(0xF5, 0x9E, 0x0B, 0x24);
/// Tauri `.stack-wrapper--locked .stack-capsule__badge { color: #fcd34d }`.
const STACK_CAPSULE_LOCKED_BADGE_TEXT: Color = Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF);
/// Tauri `.stack-capsule.has-preview` ring:
/// `0 0 0 1px rgba(59, 130, 246, 0.42)`.
const STACK_CAPSULE_PREVIEW_RING: Color = Color::from_u8(0x3B, 0x82, 0xF6, 0x6B);
/// Tauri `.stack-capsule__preview-indicator { background: rgba(59, 130, 246, 0.14) }`.
const STACK_CAPSULE_PREVIEW_INDICATOR_FILL: Color = Color::from_u8(0x3B, 0x82, 0xF6, 0x24);
/// Tauri `.stack-capsule__preview-indicator { color: #93c5fd }`.
const STACK_CAPSULE_PREVIEW_INDICATOR_TEXT: Color = Color::from_u8(0x93, 0xC5, 0xFD, 0xFF);
/// Tauri `.stack-capsule__preview-indicator { font-size: 11px; font-weight: 600 }`.
const STACK_CAPSULE_PREVIEW_INDICATOR_FONT_PX: f32 = 11.0;
const STACK_CAPSULE_PREVIEW_INDICATOR_FONT_WEIGHT: u16 = 600;
/// Tauri `.stack-capsule__preview-indicator { padding: 3px 8px }`.
const STACK_CAPSULE_PREVIEW_INDICATOR_PAD_X: f32 = 8.0;
const STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT: f32 = 20.0;
const STACK_CAPSULE_PREVIEW_INDICATOR_MIN_WIDTH: f32 = 34.0;
const STACK_CAPSULE_PREVIEW_INDICATOR_MAX_WIDTH: f32 = 82.0;
type DeviceRegionRect = (i32, i32, i32, i32);

#[inline]
fn full_client_device_region(
    viewport: bento_nano_style::Size,
    scale: f32,
) -> Option<DeviceRegionRect> {
    let scale = scale.max(0.01);
    let right = (viewport.width * scale).ceil() as i32;
    let bottom = (viewport.height * scale).ceil() as i32;
    (right > 0 && bottom > 0).then_some((0, 0, right, bottom))
}

#[inline]
fn main_region_precedes_present(
    kind: WindowKind,
    zone_drag_active: bool,
    zone_resize_active: bool,
) -> bool {
    kind == WindowKind::Main && (zone_drag_active || zone_resize_active)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StackCapsuleBloomVisual {
    recede_t: f32,
    scale: f32,
    opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StackCapsuleEmergeVisual {
    scale: f32,
    opacity: f32,
}

struct CachedLinearGradientBrush {
    top: Color,
    bottom: Color,
    brush: ID2D1LinearGradientBrush,
}

fn direct_text_halign(align: dwrite::TextAlign) -> DWRITE_TEXT_ALIGNMENT {
    match align.h {
        dwrite::HAlign::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
        dwrite::HAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        dwrite::HAlign::Trailing => DWRITE_TEXT_ALIGNMENT_TRAILING,
    }
}

fn direct_text_valign(align: dwrite::TextAlign) -> DWRITE_PARAGRAPH_ALIGNMENT {
    match align.v {
        dwrite::VAlign::Near => DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
        dwrite::VAlign::Center => DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        dwrite::VAlign::Far => DWRITE_PARAGRAPH_ALIGNMENT_FAR,
    }
}

/// M2③ (05-31, 1:1) — thickness of the expanded-panel top accent edge in
/// logical px. Matches Tauri `.bento-zone--expanded { border-top: 2px solid
/// var(--zone-accent, transparent) }` (BentoZone.css:114). Const-only so the
/// per-frame zone draw stays allocation-free (§10).
const PANEL_ACCENT_EDGE_THICKNESS_PX: f32 = 2.0;
// Tauri `ItemCard.css` uses `--font-size-xs` (11px).  The previous 14px
// runtime-frame override made short names dominate the grid while long names
// collapsed to the 8px floor, so one row visibly mixed several type scales.
const ITEM_LABEL_BASE_FONT_PX: f32 = 11.0;
const ITEM_LABEL_MIN_FONT_PX: f32 = 8.0;
const ITEM_LABEL_BOTTOM_INSET_PX: f32 = 8.0;

#[inline]
fn item_label_text_color_for_reference(pal: bento_nano_style::tokens::PaletteTauri) -> Color {
    pal.text_secondary
}

/// Frosted-backdrop rollback switch (`receipts/FROSTED-BACKDROP-SPEC.md` §
/// "Degrade ladder" #3, Wave G Mica-leak precedent). When `false` the entire
/// desktop capture + blur path is skipped — no `screencap` call, no bitmap
/// brush — and `fill_frosted_rect` collapses to a plain `fill_rounded_rect`
/// (the single flat tint, never the old double layer). Flip to `false` during
/// live verify if the real-acrylic frost misbehaves.
const FROSTED_BACKDROP: bool = true;

/// Native auxiliary HWNDs cannot reuse Main's monitor-aligned wallpaper
/// capture without making the backdrop slide independently while the window is
/// dragged. Until Windows Acrylic is actually active for that HWND, keep the
/// card surface solid and leave transparency only in the rounded outer corners.
#[inline]
fn opaque_auxiliary_surface(color: Color) -> Color {
    with_alpha(color, 1.0)
}

/// Flat fallback opacity when even the captured source bitmap is unavailable.
/// Keep enough density to mute Explorer labels without turning every Zone into
/// the opaque black block seen in the 2026-07-13 hand test.
const FROSTED_FALLBACK_MIN_ALPHA: f32 = 0.78;

#[inline]
fn frosted_fallback_underlay(tint: Color) -> Option<Color> {
    if tint.a >= FROSTED_FALLBACK_MIN_ALPHA || tint.a >= 1.0 {
        return None;
    }
    let alpha = (FROSTED_FALLBACK_MIN_ALPHA - tint.a) / (1.0 - tint.a);
    Some(with_alpha(tint, alpha.clamp(0.0, 1.0)))
}

/// Bitmap-brush opacity that reproduces CSS group opacity after the tint is
/// faded separately. For source tint alpha `a` and group opacity `p`, solving
/// `(1 - p*a) * q = p * (1 - a)` preserves the intended blur contribution.
#[inline]
fn frosted_group_backdrop_opacity(tint_alpha: f32, group_opacity: f32) -> f32 {
    let a = tint_alpha.clamp(0.0, 1.0);
    let p = group_opacity.clamp(0.0, 1.0);
    let denominator = 1.0 - p * a;
    if denominator <= f32::EPSILON {
        0.0
    } else {
        (p * (1.0 - a) / denominator).clamp(0.0, 1.0)
    }
}

/// Frosted-backdrop downsample factor (`screencap::capture_primary_workarea_blurred`).
/// `4` = quarter-res source + baked bitmap. The original half-res capture held
/// the visual target but pushed the selected-stack release budget above the
/// WS-7 25 MB Private Bytes gate on 2560px work areas; quarter-res keeps the
/// same screen-space blur radius while capping the persistent bitmap footprint.
const FROSTED_BACKDROP_DOWNSAMPLE: u32 = 4;

/// Frosted-backdrop gaussian standard deviation in DOWNSAMPLED px. Tauri uses
/// separate blur tokens (`--blur-zen: blur(20px) saturate(160%)`,
/// `--blur-expanded: blur(24px) saturate(170%)`), but nano keeps one baked
/// backdrop bitmap to preserve the strict memory budget. Bias the shared bitmap
/// to the always-visible collapsed capsule token: at downsample 4 the source
/// stddev is `20 / 4 = 5.0` (Blink maps `blur(r)` to `feGaussianBlur
/// stdDeviation = r` in CSS px). Expanded panels still get their stronger
/// 82%-alpha tint on top of this same capture.
const FROSTED_BACKDROP_STDDEV: f32 = 5.0;

/// Frosted-backdrop post-blur saturation factor (`D2D1Saturation` chained after
/// the gaussian) for Tauri dark `--blur-zen` `saturate(160%)`.
const FROSTED_BACKDROP_SATURATION_DARK: f32 = 1.6;

/// Frosted-backdrop post-blur saturation factor for Tauri light `--blur-zen`
/// `saturate(130%)`. The shared nano backdrop is re-baked when theme polarity
/// changes; it is NOT a second long-lived bitmap, so the memory ceiling stays
/// tied to one cached capture.
const FROSTED_BACKDROP_SATURATION_LIGHT: f32 = 1.3;
const AUXILIARY_OPEN_ANIMATION_MS: u32 = 180;

#[inline]
fn expanded_panel_accent_clip_rect(rect: bento_nano_style::Rect) -> bento_nano_style::Rect {
    bento_nano_style::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: PANEL_ACCENT_EDGE_THICKNESS_PX.min(rect.height.max(0.0)),
    }
}

#[inline]
fn lerp_rect_clamped(
    from: bento_nano_style::Rect,
    to: bento_nano_style::Rect,
    progress: f32,
) -> bento_nano_style::Rect {
    let t = progress.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    bento_nano_style::Rect {
        x: from.x * inv + to.x * t,
        y: from.y * inv + to.y * t,
        width: from.width * inv + to.width * t,
        height: from.height * inv + to.height * t,
    }
}

#[inline]
fn expanded_header_title_rect(
    layout: &expanded_zone_grid::ExpandedZoneLayout,
) -> bento_nano_style::Rect {
    let title_x = layout.header_icon.right() + expanded_zone_grid::HEADER_GAP;
    let title_right = (layout.header_badge.x - expanded_zone_grid::HEADER_GAP).max(title_x);
    bento_nano_style::Rect {
        x: title_x,
        y: layout.panel.y,
        width: (title_right - title_x).max(0.0),
        height: expanded_zone_grid::HEADER_BAND_HEIGHT,
    }
}

/// Move the single icon/title/badge identity row into the final PanelHeader
/// slots. The identity is painted once during morph, so it travels with the
/// shell instead of cross-fading between two visibly separate copies.
#[inline]
fn morph_zen_content_to_header(
    zen: ZonePillLayout,
    header: &expanded_zone_grid::ExpandedZoneLayout,
    progress: f32,
) -> ZonePillLayout {
    let header_title = expanded_header_title_rect(header);
    ZonePillLayout {
        rect: lerp_rect_clamped(zen.rect, header.header_band, progress),
        shadow_outer: zen.shadow_outer,
        shadow_inner: zen.shadow_inner,
        icon: lerp_rect_clamped(zen.icon, header.header_icon, progress),
        label: lerp_rect_clamped(zen.label, header_title, progress),
        badge: lerp_rect_clamped(zen.badge, header.header_badge, progress),
        radius: zen.radius,
        badge_radius: zen.badge_radius,
    }
}

#[inline]
fn moved_zone_drag_source(app: &AppState, zone_id: ZoneId) -> bool {
    let Some((dragged, _, _)) = app.zone_drag.get() else {
        return false;
    };
    if dragged != zone_id {
        return false;
    }
    app.zone_drag_origin
        .get()
        .map(|(_, _, moved)| moved)
        .unwrap_or(false)
}

#[inline]
fn zone_drag_visual_opacity(app: &AppState, zone_id: ZoneId) -> f32 {
    if moved_zone_drag_source(app, zone_id) {
        ZONE_DRAG_VISUAL_OPACITY
    } else {
        1.0
    }
}

#[inline]
fn zone_draw_layer(app: &AppState, zone: &Zone) -> u8 {
    if moved_zone_drag_source(app, zone.id) {
        2
    } else if app.zone_on_top(zone) {
        1
    } else {
        0
    }
}

#[inline]
fn collapsed_pill_display_count(app: &AppState, zone: &Zone) -> usize {
    app.zones
        .stack_member_ids(zone.id)
        .map(|members| members.len())
        .unwrap_or_else(|| zone.items.len())
}

#[inline]
fn tauri_zone_accent_color(zone_accent: Option<&str>) -> Option<Color> {
    zone_accent.and_then(parse_hex_color)
}

#[inline]
fn tauri_badge_fill(zone_accent: Option<&str>, fallback_badge_bg: Color) -> Color {
    tauri_zone_accent_color(zone_accent).unwrap_or(fallback_badge_bg)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PanelHeaderButtonChrome {
    background: Option<Color>,
    glyph: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuxiliaryActionEmphasis {
    Primary,
    Secondary,
    Danger,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AuxiliaryActionChrome {
    fill: Color,
    border: Color,
    text: Color,
}

#[inline]
fn auxiliary_action_chrome(
    palette: bento_nano_style::tokens::PaletteTauri,
    emphasis: AuxiliaryActionEmphasis,
) -> AuxiliaryActionChrome {
    let controls = palette.control_palette();
    match emphasis {
        AuxiliaryActionEmphasis::Primary => AuxiliaryActionChrome {
            fill: with_alpha(palette.accent_blue, 0.88),
            border: palette.accent_blue,
            text: controls.on_accent,
        },
        AuxiliaryActionEmphasis::Secondary => AuxiliaryActionChrome {
            fill: controls.fill,
            border: controls.border,
            text: palette.text_primary,
        },
        AuxiliaryActionEmphasis::Danger => AuxiliaryActionChrome {
            fill: with_alpha(palette.accent_red, 0.16),
            border: with_alpha(palette.accent_red, 0.42),
            text: palette.accent_red,
        },
        AuxiliaryActionEmphasis::Disabled => AuxiliaryActionChrome {
            fill: controls.disabled_fill,
            border: controls.disabled_border,
            text: controls.disabled_text,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ExpandedPanelAuxChrome {
    live_folder_fill: Color,
    live_folder_text: Color,
}

#[inline]
fn expanded_panel_aux_chrome(
    pal: bento_nano_style::tokens::PaletteTauri,
) -> ExpandedPanelAuxChrome {
    ExpandedPanelAuxChrome {
        live_folder_fill: with_alpha(pal.text_primary, 0.08),
        live_folder_text: pal.text_muted,
    }
}

#[inline]
fn panel_header_button_chrome(
    pal: bento_nano_style::tokens::PaletteTauri,
    button: PanelHeaderButtonKind,
    hovered: bool,
) -> PanelHeaderButtonChrome {
    if !hovered {
        return PanelHeaderButtonChrome {
            background: None,
            glyph: pal.text_muted,
        };
    }
    match button {
        PanelHeaderButtonKind::Search => PanelHeaderButtonChrome {
            background: Some(pal.surface_hover),
            glyph: pal.text_primary,
        },
        PanelHeaderButtonKind::Close => PanelHeaderButtonChrome {
            background: Some(with_alpha(pal.accent_red, 0.20)),
            glyph: pal.accent_red,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SettingsThemeCardChrome {
    fill: Color,
    border: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StackCapsuleBadgeChrome {
    fill: Color,
    text: Color,
}

#[inline]
fn settings_theme_card_chrome(
    pal: bento_nano_style::tokens::PaletteTauri,
    selection_progress: f32,
    is_hovered: bool,
) -> SettingsThemeCardChrome {
    let controls = pal.control_palette();
    let progress = selection_progress.clamp(0.0, 1.0);
    let idle_fill = if is_hovered {
        controls.hover_fill
    } else {
        controls.fill
    };
    let active_fill = with_alpha(pal.accent_blue, if is_hovered { 0.14 } else { 0.10 });
    let border = if progress > 0.0 || is_hovered {
        let idle_border = if is_hovered {
            controls.border
        } else {
            with_alpha(pal.accent_blue, 0.0)
        };
        Some(lerp_color(idle_border, pal.accent_blue, progress))
    } else {
        None
    };
    SettingsThemeCardChrome {
        fill: lerp_color(idle_fill, active_fill, progress),
        border,
    }
}

#[inline]
fn stack_capsule_locked_opacity(is_locked: bool) -> f32 {
    if is_locked {
        STACK_CAPSULE_LOCKED_OPACITY
    } else {
        1.0
    }
}

#[inline]
fn stack_capsule_badge_chrome(
    pal: bento_nano_style::tokens::PaletteTauri,
    is_locked: bool,
) -> StackCapsuleBadgeChrome {
    if is_locked {
        StackCapsuleBadgeChrome {
            fill: STACK_CAPSULE_LOCKED_BADGE_FILL,
            text: STACK_CAPSULE_LOCKED_BADGE_TEXT,
        }
    } else {
        StackCapsuleBadgeChrome {
            fill: with_alpha(pal.text_primary, 0.08),
            text: pal.text_primary,
        }
    }
}

#[inline]
fn stack_capsule_is_locked(app: &AppState, anchor: &Zone, member_ids: &[ZoneId]) -> bool {
    anchor.locked
        || member_ids.iter().any(|member_id| {
            app.zones
                .get(*member_id)
                .is_some_and(|member| member.locked)
        })
}

#[inline]
fn stack_capsule_has_preview(app: &AppState, anchor_id: ZoneId) -> bool {
    app.stack_tray
        .borrow()
        .as_ref()
        .map(|tray| {
            tray.anchor_zone_id == anchor_id
                && stack_tray::focused_preview_visible(tray.anchor_zone_id, tray.selected_member_id)
        })
        .unwrap_or(false)
}

#[inline]
fn stack_surface_allows_bloom(app: &AppState) -> bool {
    app.selected_zone.get().is_none()
        && app
            .stack_tray
            .borrow()
            .as_ref()
            .is_none_or(stack_tray::StackTrayState::is_bloom_preview)
}

#[inline]
fn stack_capsule_show_preview_indicator(has_preview: bool, recede_t: f32) -> bool {
    has_preview && recede_t <= f32::EPSILON
}

#[inline]
fn stack_capsule_preview_indicator_width(label: &str) -> f32 {
    let mut em = 0.0_f32;
    for ch in label.chars() {
        em += if ch.is_ascii() { 0.58 } else { 1.0 };
    }
    (em * STACK_CAPSULE_PREVIEW_INDICATOR_FONT_PX + STACK_CAPSULE_PREVIEW_INDICATOR_PAD_X * 2.0)
        .clamp(
            STACK_CAPSULE_PREVIEW_INDICATOR_MIN_WIDTH,
            STACK_CAPSULE_PREVIEW_INDICATOR_MAX_WIDTH,
        )
}

#[inline]
fn item_label_visible_name(name: &str) -> &str {
    let Some(ext) = name.get(name.len().saturating_sub(4)..) else {
        return name;
    };
    if !(ext.eq_ignore_ascii_case(".lnk") || ext.eq_ignore_ascii_case(".url")) {
        return name;
    }
    name.get(..name.len() - 4).unwrap_or(name)
}

#[inline]
fn item_label_font_size_for_width(text: &str, avail_w: f32) -> f32 {
    // Tauri ItemCard delegates to `useTextAbbrGroup`: keep the complete label
    // text and shrink toward the shared 8px floor instead of emitting `...`.
    if text.is_empty() || avail_w <= 0.0 {
        return ITEM_LABEL_BASE_FONT_PX;
    }
    let width_at_base = item_label_estimated_width(text, ITEM_LABEL_BASE_FONT_PX);
    if width_at_base <= avail_w {
        return ITEM_LABEL_BASE_FONT_PX;
    }
    (ITEM_LABEL_BASE_FONT_PX * avail_w / width_at_base)
        .floor()
        .clamp(ITEM_LABEL_MIN_FONT_PX, ITEM_LABEL_BASE_FONT_PX)
}

#[inline]
fn item_label_group_font_size<'a>(labels: impl Iterator<Item = (&'a str, f32)>) -> f32 {
    labels.fold(ITEM_LABEL_BASE_FONT_PX, |group_px, (text, avail_w)| {
        group_px.min(item_label_font_size_for_width(text, avail_w))
    })
}

#[inline]
fn item_label_estimated_width(text: &str, font_px: f32) -> f32 {
    let mut ems = 0.0_f32;
    for ch in text.chars() {
        ems += item_label_char_width_em(ch);
    }
    ems * font_px
}

#[inline]
fn item_label_char_width_em(ch: char) -> f32 {
    let cp = ch as u32;
    if (0x4E00..=0x9FFF).contains(&cp)
        || (0x3040..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
    {
        return 1.0;
    }
    if ch.is_ascii_whitespace() {
        return 0.28;
    }
    if matches!(ch, '-' | '_' | '.' | '/' | '\\' | ':' | '·' | '・') {
        return 0.3;
    }
    if ch.is_ascii_uppercase() {
        return 0.56;
    }
    if ch.is_ascii_alphanumeric() {
        return 0.49;
    }
    0.7
}

#[inline]
fn item_icon_slots_for_card(
    card_rect: bento_nano_style::Rect,
    is_wide: bool,
    scale: f32,
) -> (bento_nano_style::Rect, bento_nano_style::Rect) {
    let size = if is_wide {
        item_icon::IconSize::Wide
    } else {
        item_icon::IconSize::Standard
    };
    let container_side = size.container_px() * scale;
    let render_side = size.render_px() * scale;
    let container = bento_nano_style::Rect {
        x: card_rect.x + ((card_rect.width - container_side) * 0.5).max(0.0),
        y: card_rect.y + 8.0 * scale,
        width: container_side,
        height: container_side,
    };
    let render = bento_nano_style::Rect {
        x: container.x + ((container.width - render_side) * 0.5).max(0.0),
        y: container.y + ((container.height - render_side) * 0.5).max(0.0),
        width: render_side,
        height: render_side,
    };
    (container, render)
}

#[inline]
fn item_label_rect_for_card(
    card_rect: bento_nano_style::Rect,
    scale: f32,
    label_font_px: f32,
) -> bento_nano_style::Rect {
    let label_w = (card_rect.width - 8.0 * scale).max(0.0);
    let label_h = label_font_px * 1.4 * scale;
    let label_bottom = card_rect.bottom() - ITEM_LABEL_BOTTOM_INSET_PX * scale;
    bento_nano_style::Rect {
        x: card_rect.x + 4.0 * scale,
        y: (label_bottom - label_h).max(card_rect.y),
        width: label_w,
        height: label_h,
    }
}

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
        // system fonts. Tauri's CSS stack starts with "Segoe UI"; DWrite's
        // system fallback covers CJK glyphs when needed. On a stripped SKU it
        // falls back through Microsoft YaHei UI / Tahoma. ("MS Shell Dlg 2" is
        // a GDI alias DWrite's FindFamilyName cannot resolve — it would always
        // probe-miss — and the resolver's universal tail is already Tahoma, so
        // it is omitted as dead weight.)
        let ui_family: &'static str = dwrite::resolve_default_family(
            dwrite::FontRole::Ui,
            &["Segoe UI", "Microsoft YaHei UI", "Tahoma"],
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
            main_region_installed: false,
            main_region_signature: SmallVec::new(),
            ellipsis_sign: None,
            bloom_petal_ellipsis_sign: None,
            monospace_format: None,
            monospace_ellipsis_sign: None,
            dashed_stroke_style: None,
            linear_gradient_brush: None,
            pill_title_ellipsis_sign: None,
            stack_capsule_title_shrink: None,
            width,
            height,
            utf16_scratch: SmallVec::new(),
            mask_scratch: String::new(),
            // 1.0 = 96 DPI baseline. `render()` overwrites this each frame
            // from `WindowState.dpi` before any draw call observes it.
            base_scale: 1.0,
            logical_transform_override: None,
            auxiliary_open_started_ms: None,
            icon_bitmaps: HashMap::new(),
            icon_bitmap_failures: HashSet::new(),
            image_file_bitmaps: HashMap::new(),
            image_file_failures: HashSet::new(),
            svg_cache: SvgCache::default(),
            debug_overlay_started_at: Instant::now(),
            hwnd,
            device_gen: bento_nano_platform::device_generation(),
            // Frosted-backdrop — no snapshot yet; `backdrop_dirty = true` so the
            // first Main-overlay paint captures the desktop. Brush is per-frame.
            backdrop: None,
            backdrop_dirty: true,
            backdrop_saturation: FROSTED_BACKDROP_SATURATION_DARK,
            backdrop_brush: None,
        })
    }

    /// Frosted-backdrop — mark the cached desktop snapshot stale so the next
    /// Main-overlay `render()` re-captures + re-blurs the primary work area.
    /// The shell calls this on `WM_DISPLAYCHANGE` (resolution / monitor
    /// topology), `WM_SETTINGCHANGE` (wallpaper arrives as SPI_SETDESKWALLPAPER),
    /// and the ToggleMain show transition (the desktop behind the overlay may
    /// have changed while it was hidden). Cheap flag flip — the actual capture
    /// is deferred to the paint hot path (spec §10: no capture off the frame).
    #[inline]
    pub fn mark_backdrop_dirty(&mut self) {
        self.backdrop_dirty = true;
    }

    pub fn start_auxiliary_open_animation(&mut self, now_ms: u32) {
        self.auxiliary_open_started_ms = Some(now_ms);
    }

    pub fn auxiliary_open_animation_pending(&self, now_ms: u32) -> bool {
        self.auxiliary_open_started_ms
            .is_some_and(|started| now_ms.wrapping_sub(started) < AUXILIARY_OPEN_ANIMATION_MS)
    }

    pub fn settle_auxiliary_open_animation(&mut self, now_ms: u32) -> bool {
        let Some(started) = self.auxiliary_open_started_ms else {
            return false;
        };
        if now_ms.wrapping_sub(started) < AUXILIARY_OPEN_ANIMATION_MS {
            return false;
        }
        self.auxiliary_open_started_ms = None;
        true
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
        // Drop any cached backdrop with the backbuffer so a hibernated renderer
        // retains neither GPU bitmap nor brush.
        self.backdrop_brush = None;
        self.backdrop = None;
        self.backdrop_dirty = true;
        self.surface = None;
        self.comp.release_chain();
    }

    /// Recreate the backbuffer + D2D surface after `release_swap_chain`.
    /// Idempotent: returns `Ok(())` immediately if already resident.
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
        // G5 — the dashed stroke style was created from the (now-rebuilt) D2D
        // factory; drop it so the next minimal-capsule paint re-creates it
        // against the recovered factory. Cheap one-off rebuild, not per-frame.
        self.dashed_stroke_style = None;
        self.linear_gradient_brush = None;
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

        // Frosted-backdrop — clear the per-frame brush FIRST so an unrelated
        // auxiliary frame, a hibernated surface, or a `None` backdrop can never
        // reuse a stale brush (spec §10 / degrade ladder).
        self.backdrop_brush = None;

        // Only Main shares the exact origin and extent of the captured monitor
        // work area. Settings is a movable panel-sized native popup; sampling a
        // work-area bitmap with a zero translation there detached the wallpaper
        // from the window and produced dark/square drag artifacts. Its dense
        // theme surface is the deliberate flat-tint degradation path.
        let uses_frosted_backdrop = kind == WindowKind::Main;
        let backdrop_saturation = frosted_backdrop_saturation_for_palette(app.active_theme_tauri());
        if FROSTED_BACKDROP
            && uses_frosted_backdrop
            && frosted_backdrop_saturation_recapture_needed(
                kind,
                self.backdrop_saturation,
                backdrop_saturation,
            )
        {
            self.backdrop_dirty = true;
        }

        // Frosted-backdrop capture (real acrylic) — Main only, and only when
        // `backdrop_dirty`. This MUST run
        // before the frame's
        // `ctx.BeginDraw()` below: `capture_primary_workarea_blurred` does its
        // OWN BeginDraw/EndDraw internally to bake the blur, and BeginDraw
        // cannot be nested on one device context. The capture is on-demand —
        // steady-state frames reuse the cached bitmap (zero per-frame capture).
        // Degrade-not-panic (spec § "Degrade ladder"): on `Err` we drop the
        // backdrop to `None` (→ flat tint) and STILL clear the dirty flag so we
        // don't re-attempt a failing capture every frame; the next dirty event
        // (display / wallpaper / show) retries.
        if FROSTED_BACKDROP && uses_frosted_backdrop && self.backdrop_dirty {
            let captured = match self.surface.as_ref() {
                Some(surface) => Some(capture_primary_workarea_blurred(
                    &surface.ctx,
                    self.hwnd,
                    FROSTED_BACKDROP_DOWNSAMPLE,
                    FROSTED_BACKDROP_STDDEV,
                    backdrop_saturation,
                )),
                // Hibernated surface — leave dirty set so the next resident
                // paint captures; nothing to do this frame.
                None => None,
            };
            if let Some(result) = captured {
                self.backdrop = match result {
                    Ok(backdrop) => Some(backdrop),
                    Err(error) => {
                        tracing::warn!(
                            target: "bentodesk::render",
                            %error,
                            "frosted backdrop capture unavailable; using flat tint"
                        );
                        None
                    }
                };
                self.backdrop_saturation = backdrop_saturation;
                self.backdrop_dirty = false;
            }
        }

        // Frosted-backdrop — build the per-frame bitmap brush ONCE for Main from
        // the cached `backdrop` (spec §10: one cheap brush build
        // per frame, no capture). Done here, before the long-lived `surface` borrow
        // below, so the `&mut self.backdrop_brush` write does not race the
        // immutable `surface`/`ctx` borrow that the rest of the frame holds.
        // `CreateBitmapBrush` does not need an active `BeginDraw`. A `None`
        // backdrop / build failure leaves `backdrop_brush = None` → flat tint.
        if FROSTED_BACKDROP && uses_frosted_backdrop {
            let brush = match self.surface.as_ref() {
                Some(surface) => self.build_backdrop_brush(&surface.ctx),
                None => None,
            };
            self.backdrop_brush = brush;
        }

        self.logical_transform_override = None;

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
            // automatically. SVG paths re-establish the current logical
            // transform because their per-glyph transforms also need the
            // projection.
            let base = base_scale_matrix(scale);
            ctx.SetTransform(&base);
        }

        let auxiliary_open_transform_active = kind == WindowKind::IconPicker
            && self.auxiliary_open_animation_pending(unsafe {
                windows_sys::Win32::System::SystemInformation::GetTickCount()
            });
        if auxiliary_open_transform_active {
            // SAFETY: GetTickCount is total and thread-safe.
            let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
            let started = self.auxiliary_open_started_ms.unwrap_or(now_ms);
            let raw = now_ms.wrapping_sub(started) as f32 / AUXILIARY_OPEN_ANIMATION_MS as f32;
            let scale = 0.965 + 0.035 * animator::ease_out_cubic(raw);
            let transform = scale_about_rect_center_matrix(
                self.base_scale,
                picker_geometry::picker_panel(app.viewport),
                scale,
            );
            self.set_logical_transform_override(Some(transform))?;
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
            WindowKind::ContextMenu => {
                self.draw_context_menu_window(app)?;
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
        if auxiliary_open_transform_active {
            self.set_logical_transform_override(None)?;
        }
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
        // field + helper stay alive for Settings / the picker pop-up that lets
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
        // Zone/item menus are transient chrome on the already-resident Main
        // surface. Reusing this renderer avoids a second DComp swap chain and
        // keeps the right-click path inside the strict private-memory budget.
        if app.active_context_menu.borrow().is_some() {
            self.draw_context_menu_window(app)?;
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
        let region_precedes_present = main_region_precedes_present(
            kind,
            app.zone_drag.get().is_some(),
            app.zone_resize.get().is_some(),
        );
        if region_precedes_present {
            // Expand the input/visual clip before submitting the first moved
            // frame. Doing this after Present leaves that frame clipped to the
            // old capsule rect and produces the one-frame blank/flash seen at
            // drag latch.
            self.apply_main_click_through_region(app);
        }
        self.comp.present()?;

        // P0 click-through (CLICKTHROUGH-FIX-VALIDATED.md, 2026-06-02) — clip
        // the Main HWND's window region to the UNION of every painted
        // interactive surface. `WS_EX_TRANSPARENT` is INERT under
        // `WS_EX_NOREDIRECTIONBITMAP` (window.rs:254-256) and `HTTRANSPARENT`
        // alone does NOT reach the bare desktop, so blank pixels of the
        // full-work-area overlay otherwise eat every click. Region clipping
        // keeps DComp / `NoRedirectionBitmap` (spec §4.1) untouched: blank
        // areas fall OUTSIDE the window so clicks land on the desktop
        // natively. Main HWND only — aux dialogs are real windows that own
        // their whole client rect. Stable exact regions apply after present;
        // an active move/resize installs its full-client region before present
        // so the first moved frame cannot be clipped to stale geometry. The
        // Win32 path degrades silently (no panic).
        if kind == WindowKind::Main && !region_precedes_present {
            self.apply_main_click_through_region(app);
        }
        self.record_debug_overlay_frame(app, kind, frame_started_at);
        Ok(())
    }

    /// P0 click-through — set the Main HWND window region to the painted-chrome
    /// union so blank desktop pixels pass clicks through natively.
    ///
    /// The region rects come from [`chrome_region_rects`] (the single source of
    /// truth, mirroring `bento-nano-shell::ui::main_nchittest_kind`), are
    /// expressed in logical DIP, and are converted to PHYSICAL device px here by
    /// multiplying by `base_scale` (= dpi/96; the user runs 150% → ×1.5).
    /// `SetWindowRgn` wants device px, so this conversion MUST happen or the
    /// region misaligns at non-100% DPI.
    ///
    /// GDI lifecycle: each rect becomes a temporary `HRGN` that is OR-combined
    /// into one accumulator; the temporaries are `DeleteObject`-freed after the
    /// combine, and the FINAL accumulator is handed to `SetWindowRgn`, which
    /// TAKES OWNERSHIP (we never `DeleteObject` it; the system frees the prior
    /// region). When NOTHING is painted, an EMPTY 0×0 region is set so the WHOLE
    /// desktop is click-through — the region is NEVER left NULL (NULL = whole
    /// window catches = the original bug).
    ///
    /// Spec §10 hot path: the rect set is a stack-inlined `SmallVec<[_; 16]>`
    /// (no heap unless a process pins >16 zones), N small GDI regions (which
    /// `SetWindowRgn` requires), one `SetWindowRgn`. No `unwrap`/`expect`/`panic`
    /// — every Win32 failure degrades to leaving the previous region (the
    /// ghost-layer passthrough toggle is the belt-and-suspenders fallback).
    fn apply_main_click_through_region(&mut self, app: &AppState) {
        // windows-sys 0.59 places ALL of these — including `SetWindowRgn`
        // (which the docs file under user32) — in `Graphics::Gdi`. Verified by
        // compile: `SetWindowRgn` is NOT in `UI::WindowsAndMessaging` here.
        use windows_sys::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
        };

        // DIP → physical device px. `base_scale` is `dpi/96` (set once per
        // frame at the top of `render`); guard against a degenerate <=0 scale.
        let scale = self.base_scale.max(0.01);
        let mut signature: SmallVec<[DeviceRegionRect; 16]> = SmallVec::new();
        if app.zone_drag.get().is_some() || app.zone_resize.get().is_some() {
            // W13-B — while mouse capture owns an active move/resize, install
            // one stable full-client region. The old code rebuilt SetWindowRgn
            // after every DComp present; the moving visual was clipped by the
            // previous geometry for a frame, producing blank/blue flashes and
            // excessive GDI work. Mouse-up clears the drag state, and the next
            // paint restores the exact chrome-only region.
            if let Some(full_client) = full_client_device_region(app.viewport, scale) {
                signature.push(full_client);
            }
        } else {
            let rects = chrome_region_rects(app);
            for r in rects.iter() {
                // Convert DIP rect → physical px, rounding outward so a painted
                // surface is never under-covered (left/top floor, right/bottom
                // ceil). Clamp non-positive extents away (skip empty rects).
                let left = (r.x * scale).floor() as i32;
                let top = (r.y * scale).floor() as i32;
                let right = (r.right() * scale).ceil() as i32;
                let bottom = (r.bottom() * scale).ceil() as i32;
                if right > left && bottom > top {
                    signature.push((left, top, right, bottom));
                }
            }
        }
        if self.main_region_installed && self.main_region_signature == signature {
            return;
        }

        // Accumulator region. Start EMPTY (0×0) so the "no painted surface"
        // case leaves the whole desktop click-through. `CreateRectRgn` returns
        // a null handle on GDI failure — treat that as "skip region surgery
        // this frame" rather than panic / NULL-region (which would re-arm the
        // whole-window-catches bug).
        // SAFETY: GDI region creation is always callable; null is checked.
        let combined = unsafe { CreateRectRgn(0, 0, 0, 0) };
        if combined.is_null() {
            return;
        }

        let mut built_all_parts = true;
        for &(left, top, right, bottom) in signature.iter() {
            // SAFETY: GDI region creation is always callable; null is checked
            // so a single allocation failure just drops that one rect.
            let part = unsafe { CreateRectRgn(left, top, right, bottom) };
            if part.is_null() {
                built_all_parts = false;
                continue;
            }
            // SAFETY: `combined` and `part` are both live, non-null HRGNs;
            // RGN_OR (an `i32` = `RGN_COMBINE_MODE`) unions `part` into
            // `combined` in place.
            unsafe {
                CombineRgn(combined, combined, part, RGN_OR);
                // `part` was copied into `combined`; free the temporary HRGN.
                DeleteObject(part);
            }
        }

        // Hand the final region to the system. `SetWindowRgn` TAKES OWNERSHIP
        // of `combined` (do NOT DeleteObject it) and frees the window's prior
        // region. bRedraw = FALSE: DComp composites independently, so no
        // invalidate is needed and we avoid a redundant repaint (spec §10).
        // `self.hwnd` is a `windows` 0.58 `HWND(*mut c_void)`; `.0` is the raw
        // pointer that windows-sys 0.59 `SetWindowRgn` expects (same ABI — see
        // `bento-nano-platform::window::to_windows_hwnd`, the inverse bridge).
        // SAFETY: `self.hwnd` is the live Main HWND stashed at `create`;
        // `combined` is a valid HRGN whose ownership transfers to the system.
        // bredraw = 0 (FALSE).
        let applied = unsafe { SetWindowRgn(self.hwnd.0, combined, 0) };
        if applied != 0 {
            self.main_region_signature = signature;
            self.main_region_installed = built_all_parts;
        } else {
            // SAFETY: SetWindowRgn did not take ownership when it failed.
            unsafe {
                DeleteObject(combined);
            }
        }
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
        self.pill_title_ellipsis_sign = None;
        self.bloom_petal_ellipsis_sign = None;
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
        use bento_nano_style::i18n_zh_cn::ids;

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
        if state.is_bloom_preview() {
            let Some(member_index) = member_ids
                .iter()
                .position(|member_id| *member_id == state.selected_member_id)
            else {
                return Ok(());
            };
            let Some(preview_zone) = app.zones.get(state.selected_member_id) else {
                return Ok(());
            };
            let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, member_count);
            let Some(petal) = petals.get(member_index).copied() else {
                return Ok(());
            };
            let preview =
                stack_tray::focused_bloom_preview_rect(app.viewport, petal, &petals, preview_zone);
            return self.draw_focused_preview_overlay(app, preview_zone, preview, chrome);
        }
        let tray = stack_tray::stack_tray_rect(app.viewport, anchor, member_count);
        let tray_shadow = stack_tray::panel_shadow_rect(tray, chrome.panel_shadow);
        self.fill_rounded_rect(tray_shadow, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(tray, chrome.panel_background, chrome.panel_radius)?;

        self.draw_text_no_wrap_with_style(
            bento_nano_style::t(ids::STACK_MEMBERS_LABEL),
            stack_tray::stack_tray_header_title_rect(app.viewport, anchor, member_count),
            chrome.text_primary,
            stack_tray::TRAY_TITLE_FONT_PX,
            stack_tray::TRAY_TITLE_FONT_WEIGHT,
            stack_tray::TRAY_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        let count_badge =
            stack_tray::stack_tray_header_count_rect(app.viewport, anchor, member_count);
        if count_badge.width > 0.0 && count_badge.height > 0.0 {
            self.fill_rounded_rect(
                count_badge,
                with_alpha(chrome.text_accent, 0.30),
                bento_nano_style::BorderRadius::all(count_badge.height * 0.5),
            )?;
            let count_label = format_small_count(member_count);
            let count_text_rect = bento_nano_style::Rect {
                x: count_badge.x + stack_tray::TRAY_HEADER_COUNT_BADGE_PAD_X_PX,
                y: count_badge.y,
                width: (count_badge.width - stack_tray::TRAY_HEADER_COUNT_BADGE_PAD_X_PX * 2.0)
                    .max(0.0),
                height: count_badge.height,
            };
            self.draw_text_no_wrap_with_style(
                count_label.as_str(),
                count_text_rect,
                chrome.text_primary,
                stack_tray::TRAY_COUNT_FONT_PX,
                stack_tray::TRAY_COUNT_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let dissolve = stack_tray::stack_tray_dissolve_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(dissolve, chrome.danger_background, chrome.button_radius)?;
        self.draw_icon_glyph(
            "trash",
            centered_square_rect(dissolve, 14.0),
            chrome.text_primary,
        )?;
        let close = stack_tray::stack_tray_close_rect(app.viewport, anchor, member_count);
        self.fill_rounded_rect(close, chrome.button_background, chrome.button_radius)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 13.0), chrome.text_primary)?;

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
            self.draw_icon_glyph(member.icon.as_ref(), icon_rect, chrome.text_primary)?;
            self.draw_text_no_wrap_with_style(
                member.display_title(),
                bento_nano_style::Rect {
                    x: row_rect.x + 44.0,
                    y: row_rect.y + 6.0,
                    width: (row_rect.width - 128.0).max(0.0),
                    height: 17.0,
                },
                chrome.text_primary,
                stack_tray::TRAY_MEMBER_NAME_FONT_PX,
                stack_tray::TRAY_MEMBER_NAME_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            let item_count = member.items.len();
            let item_label = format_small_count(item_count);
            let meta_count = stack_tray::stack_tray_member_meta_count_rect(row_rect);
            self.draw_text_no_wrap_with_style(
                item_label.as_str(),
                meta_count,
                chrome.text_muted,
                stack_tray::TRAY_MEMBER_META_FONT_PX,
                stack_tray::TRAY_MEMBER_META_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            let detach =
                stack_tray::stack_tray_detach_rect(app.viewport, anchor, member_count, row_index);
            self.fill_rounded_rect(detach, chrome.button_background, chrome.button_radius)?;
            self.draw_icon_glyph(
                "arrow_right",
                centered_square_rect(detach, 13.0),
                chrome.text_primary,
            )?;
        }

        let status_rect = stack_tray::stack_tray_status_rect(tray);
        if member_count > stack_tray::TRAY_VISIBLE_ROW_LIMIT {
            let hidden = member_count - stack_tray::TRAY_VISIBLE_ROW_LIMIT;
            let hidden_label = format_small_count(hidden);
            self.draw_text_no_wrap_with_style(
                "+",
                stack_tray::stack_tray_status_prefix_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                hidden_label.as_str(),
                stack_tray::stack_tray_status_count_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(ids::STACK_MORE_MEMBERS),
                stack_tray::stack_tray_status_suffix_rect(status_rect),
                chrome.text_muted,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        } else if app.stack_tray_drag.get().is_some() {
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(ids::STACK_REORDER_HINT),
                status_rect,
                chrome.text_accent,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        } else if let Some(status) = state.status.as_ref() {
            self.draw_text_no_wrap_with_style(
                status.as_str(),
                status_rect,
                chrome.text_accent,
                stack_tray::TRAY_STATUS_FONT_PX,
                stack_tray::TRAY_STATUS_FONT_WEIGHT,
                stack_tray::TRAY_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
            )?;
        }

        if !stack_tray::focused_preview_visible(anchor.id, selected_id) {
            return Ok(());
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
        self.draw_text_no_wrap_with_style(
            bento_nano_style::t(ids::FOCUSED_PREVIEW_TITLE),
            bento_nano_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 12.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_accent,
            stack_tray::PREVIEW_EYEBROW_FONT_PX,
            stack_tray::PREVIEW_EYEBROW_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_zone.display_title(),
            bento_nano_style::Rect {
                x: preview.x + 16.0,
                y: preview.y + 36.0,
                width: preview.width - 32.0,
                height: 18.0,
            },
            chrome.text_primary,
            stack_tray::PREVIEW_TITLE_FONT_PX,
            stack_tray::PREVIEW_TITLE_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        let preview_w = format_small_count(preview_zone.w as usize);
        let preview_h = format_small_count(preview_zone.h as usize);
        let preview_count = format_small_count(preview_zone.items.len());
        self.draw_text_no_wrap_with_style(
            preview_w.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 0),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            "×",
            stack_tray::focused_preview_meta_mark_rect(preview, 0),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_h.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 1),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            bento_nano_style::t(ids::STACK_DIMENSION_SEPARATOR),
            stack_tray::focused_preview_meta_mark_rect(preview, 1),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            preview_count.as_str(),
            stack_tray::focused_preview_meta_number_rect(preview, 2),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            bento_nano_style::t(ids::BULK_MANAGER_COL_ITEMS),
            stack_tray::focused_preview_meta_suffix_rect(preview),
            chrome.text_muted,
            stack_tray::PREVIEW_META_FONT_PX,
            stack_tray::PREVIEW_META_FONT_WEIGHT,
            stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )?;
        if preview_zone.items.is_empty() {
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(ids::FOCUSED_PREVIEW_EMPTY),
                bento_nano_style::Rect {
                    x: preview.x + 16.0,
                    y: preview.y + 92.0,
                    width: preview.width - 32.0,
                    height: 18.0,
                },
                chrome.text_muted,
                stack_tray::PREVIEW_EMPTY_FONT_PX,
                stack_tray::PREVIEW_EMPTY_FONT_WEIGHT,
                stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
                dwrite::TextAlign::DEFAULT,
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
                self.draw_text_no_wrap_with_style(
                    item.name.as_ref(),
                    bento_nano_style::Rect {
                        x: row.x + 8.0,
                        y: row.y + 2.0,
                        width: row.width - 16.0,
                        height: 15.0,
                    },
                    chrome.text_primary,
                    stack_tray::PREVIEW_ITEM_FONT_PX,
                    stack_tray::PREVIEW_ITEM_FONT_WEIGHT,
                    stack_tray::PREVIEW_TEXT_LINE_HEIGHT,
                    dwrite::TextAlign::DEFAULT,
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
    fn draw_inline_zone_search(
        &mut self,
        app: &AppState,
        panel: bento_nano_style::Rect,
        query: &str,
    ) -> Result<(), RenderError> {
        let pal = app.active_theme_tauri();
        // SAFETY: GetTickCount is total and thread-safe.
        let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let reveal = app
            .zone_search_animation_progress_at(now_ms)
            .clamp(0.0, 1.0);
        if reveal <= f32::EPSILON {
            return Ok(());
        }
        let final_input = search_bar::zone_inline_rect(panel);
        let input = bento_nano_style::Rect {
            x: final_input.right() - final_input.width * reveal,
            width: final_input.width * reveal,
            ..final_input
        };
        self.fill_rounded_rect(
            input,
            fade_color(pal.surface_subtle, reveal),
            bento_nano_style::BorderRadius::all(8.0),
        )?;
        self.stroke_rounded_rect(
            input,
            with_alpha(pal.accent_blue, 0.78 * reveal),
            bento_nano_style::BorderRadius::all(8.0),
            1.0,
        )?;
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            bento_nano_style::Rect {
                x: input.x + 10.0,
                y: input.y + 11.0,
                width: 14.0,
                height: 14.0,
            },
            fade_color(pal.text_muted, reveal),
        )?;
        self.draw_text_no_wrap_with_style(
            if query.is_empty() {
                if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                    "搜索项目…"
                } else {
                    "Search items…"
                }
            } else {
                query
            },
            bento_nano_style::Rect {
                x: input.x + 32.0,
                y: input.y,
                width: (input.width - 66.0).max(0.0),
                height: input.height,
            },
            if query.is_empty() {
                fade_color(pal.text_muted, reveal)
            } else {
                fade_color(pal.text_primary, reveal)
            },
            12.0,
            400,
            1.4,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if !query.is_empty() && reveal > 0.55 {
            let clear = search_bar::zone_inline_clear_rect(panel);
            self.draw_icon_glyph(
                IconKind::X.as_str(),
                inset_rect(clear, 4.0),
                fade_color(pal.text_muted, reveal),
            )?;
        }
        Ok(())
    }

    fn draw_focused_preview_overlay(
        &mut self,
        app: &AppState,
        zone: &Zone,
        preview: bento_nano_style::Rect,
        chrome: stack_tray::StackTrayChrome,
    ) -> Result<(), RenderError> {
        let pal = app.active_theme_tauri();
        let palette = app.active_theme_palette();
        let radius = app.active_theme_radius_tauri();
        self.fill_frosted_rect(preview, chrome.preview_background, chrome.panel_radius)?;
        self.stroke_rounded_rect(preview, pal.border_expanded, chrome.panel_radius, 1.0)?;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: preview.x,
                y: preview.y,
                width: preview.width,
                height: 2.0,
            },
            pal.accent_blue,
            bento_nano_style::BorderRadius::all(radius.expanded),
        )?;

        let icon_rect = bento_nano_style::Rect {
            x: preview.x + 14.0,
            y: preview.y + 10.0,
            width: 28.0,
            height: 28.0,
        };
        self.fill_rounded_rect(icon_rect, pal.surface_subtle, chrome.button_radius)?;
        self.draw_icon_glyph(
            zone.icon.as_ref(),
            centered_square_rect(icon_rect, 18.0),
            pal.text_primary,
        )?;
        self.draw_text_no_wrap_with_style(
            zone.display_title(),
            bento_nano_style::Rect {
                x: icon_rect.right() + 10.0,
                y: preview.y + 10.0,
                width: (preview.width - 136.0).max(48.0),
                height: 28.0,
            },
            pal.text_primary,
            13.0,
            600,
            1.35,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        let search = stack_tray::focused_bloom_preview_search_rect(preview);
        let close = stack_tray::focused_bloom_preview_close_rect(preview);
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            centered_square_rect(search, 16.0),
            pal.text_muted,
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close, 16.0),
            pal.text_muted,
        )?;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: preview.x,
                y: preview.y + 47.0,
                width: preview.width,
                height: 1.0,
            },
            with_alpha(bento_nano_style::Color::WHITE, 0.05),
            bento_nano_style::BorderRadius::ZERO,
        )?;

        let search_active = app.zone_search_target.get() == Some(zone.id);
        let search_reveal = if search_active {
            // SAFETY: GetTickCount is total and thread-safe.
            app.zone_search_animation_progress_at(unsafe {
                windows_sys::Win32::System::SystemInformation::GetTickCount()
            })
        } else {
            0.0
        };
        let search_item_offset = search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * search_reveal;
        let search_state = app.search_bar.borrow();
        let search_query = search_state.query.as_str();
        if search_active {
            self.draw_inline_zone_search(app, preview, search_query)?;
        }

        let item_chrome = item_card::ItemCardChrome::from_tokens(
            palette,
            app.active_theme_radius(),
            pal.surface_subtle,
            item_label_text_color_for_reference(pal),
            pal.text_primary,
            pal.surface_hover,
            pal.border_hover,
        );
        // SAFETY: `GetTickCount` is total and thread-safe. One sample keeps all
        // preview cards on the same hover/press frame.
        let anim_now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        let item_hover = app.item_hover.get();
        let item_drag = app.item_drag.borrow();
        let item_label_group_px = {
            let mut label_flow_slot = 0;
            item_label_group_font_size(zone.items.iter().filter_map(|item| {
                if search_active
                    && !search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
                {
                    return None;
                }
                let (rect, next_slot) = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
                    zone,
                    preview,
                    label_flow_slot,
                    item.is_wide,
                    search_item_offset,
                );
                label_flow_slot = next_slot;
                (rect.width > 0.0 && rect.height > 0.0).then_some((
                    item_label_visible_name(item.name.as_ref()),
                    (rect.width - 8.0).max(0.0),
                ))
            }))
        };
        let mut flow_slot = 0;
        let mut visible_item_count = 0usize;
        for item in &zone.items {
            if search_active
                && !search_bar::zone_item_matches_query(item.name.as_ref(), search_query)
            {
                continue;
            }
            visible_item_count += 1;
            let (rect, next_slot) = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
                zone,
                preview,
                flow_slot,
                item.is_wide,
                search_item_offset,
            );
            flow_slot = next_slot;
            if rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let is_dragged_source = item_drag
                .as_ref()
                .is_some_and(|drag| drag.zone_id == zone.id && drag.item_id == item.id);
            let card_key = (zone.id, item.id);
            let (hover_raw, press_t) = if is_dragged_source {
                (0.0, 0.0)
            } else {
                item_hover.sample(card_key, anim_now_ms)
            };
            let hover_t = if is_dragged_source || item.file_missing {
                0.0
            } else {
                hover_raw
            };
            let item_scale = if is_dragged_source {
                1.0
            } else {
                item_card::card_scale_for(hover_raw, press_t)
            };
            self.draw_item_card(
                item,
                rect,
                if is_dragged_source {
                    item_chrome.drag_source_background
                } else if item.file_missing {
                    item_chrome.missing_background
                } else {
                    item_chrome.normal_background
                },
                &item_chrome,
                hover_t,
                !is_dragged_source && item_hover.press_held(card_key),
                item_scale,
                item_label_group_px,
                1.0,
            )?;
        }
        if search_active && visible_item_count == 0 {
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SEARCH_EMPTY),
                bento_nano_style::Rect {
                    x: preview.x + expanded_zone_grid::HEADER_INSET_X,
                    y: preview.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + search_item_offset,
                    width: (preview.width - expanded_zone_grid::HEADER_INSET_X * 2.0).max(0.0),
                    height: 28.0,
                },
                pal.text_muted,
                12.0,
                400,
                1.4,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        Ok(())
    }

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
            // Current Tauri ItemCard.css uses the secondary-text token so the
            // uniform label rail does not compete with the panel title.
            item_label_text_color_for_reference(pal),
            pal.text_primary,
            pal.surface_hover,
            pal.border_hover,
        );
        // P1.3 / P2.1 (2026-06-02 real-blur inversion) — the idle expanded
        // panel tint is `--surface-expanded` rgba(12,12,18,0.82), the token
        // Tauri's `.bento-zone--expanded { background: var(--surface-expanded) }`
        // actually uses (NOT `--surface-dialog`, which is reserved for the
        // Dialog/Settings primitive). The V-9 round-4 ruling that pinned 0.92
        // (`surface_dialog`) PREDATES the real D2D gaussian+saturation backdrop
        // (`screencap::capture_primary_workarea_blurred`): at 0.92α only ~8% of
        // the blur shows through, masking even a correct frost — exactly the
        // "完全不一样" delta. With a real backdrop the 0.82α is correct: it lets
        // the required ~18% wallpaper-bleed through so the blur(24px) saturate(1.7)
        // reads. The palette test (`surface_dialog.a > surface_expanded.a`) still
        // holds. `zone_fill_active` (active-drag accent) stays near-opaque.
        let zone_fill_idle = pal.surface_expanded;
        let zone_fill_active = with_alpha(palette.accent, 0.92);
        let expanded_panel_aux = expanded_panel_aux_chrome(pal);
        let zone_live_folder_fill = expanded_panel_aux.live_folder_fill;
        let zone_live_folder_text = expanded_panel_aux.live_folder_text;
        // #4 (2026-06-02) — the stack-anchor halo / "Stack ×N" badge / "Peek:"
        // row fills were removed (no Tauri reference); their colour bindings
        // (stack_shadow / stack_wrapper_halo / stack_badge_fill / stack_peek_fill)
        // went with them.
        let zone_drop_target_glow = with_alpha(palette.accent_hover, 0.30);
        let drop_preview_fill = with_alpha(palette.accent, 0.20);
        let drop_preview_core = with_alpha(palette.accent_hover, 0.34);
        // The expanded panel and morph endpoint share the authored Tauri
        // surface radius. `RadiusTokens::lg` is the widget scale (8 DIP in the
        // default theme), not `.bento-zone--expanded`'s 16-DIP radius.
        let radius = BorderRadius::all(app.active_theme_radius_tauri().expanded);
        let active_id = app
            .zone_drag
            .get()
            .map(|t| t.0)
            .or_else(|| app.zone_resize.get().map(|t| t.0));
        let zone_search_target = app.zone_search_target.get();
        let zone_search_query = app.search_bar.borrow().query.clone();
        // #5 (2026-06-02) — `active_id` (drag OR resize) drives the active-fill
        // tint / drop-target highlight; it must NOT force the expanded body.
        // A RESIZE can only be armed on an already-expanded panel
        // (`hit_test_zone_resize_corner` gates on `zone_pill_body_visible`),
        // so only a resize may force `pill_body_visible`. A DRAG of a
        // COLLAPSED pill must keep it a pill that follows the cursor (Tauri
        // drags the capsule itself) — forcing the body there made the pill
        // "disappear" into its mostly-empty 480×432 expanded body. The hit /
        // chrome SSoTs (`effective_zone_hit_rect` / `effective_zone_chrome_rect`)
        // already key off `zone_pill_body_visible`, so dropping the drag-force
        // restores paint == hit. The resize-force itself now lives inside the
        // shared `AppState::zone_pill_body_visible` SSoT.
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
        // Z-order — three fixed passes. Expanded/morphing zones form the normal
        // TOP layer, collapsed pills are the BOTTOM layer, and the actively
        // moved capsule is painted last. This matches Tauri's `Z_ZONE_DRAG`
        // contract: the complete 70%-opaque source capsule stays above every
        // candidate until mouse-up; stack scoring runs only on release and never
        // changes paint order or adds an early merge ring. With the
        // dense 4×4 grid a panel's 480×432 footprint overlaps the pills of zones
        // a row below it, so a single zone-order pass let a later pill overpaint
        // an earlier expanded panel (the bright count badges bled through the
        // dark frosted surface — a Tauri `.bento-zone--expanded` z-index break).
        // Fix: iterate three fixed passes — no per-frame Vec / heap allocation.
        // `zone_draw_layer` preserves the shared `AppState::zone_on_top` SSoT
        // for idle zones and adds only the transient drag override.
        for draw_layer in [0_u8, 1, 2] {
            for zone in app.zones.iter() {
                if !zone.is_visible() || zone.is_stacked_child() {
                    continue;
                }
                if zone_draw_layer(app, zone) != draw_layer {
                    continue;
                }
                // Wave C (05-20 visual parity) — collapsed pill render path.
                // #4 (2026-06-02): a COLLAPSED stack anchor renders as the compact
                // stack pill too (count badge = member count); every zone whose
                // `body_visible_for_mode` is false renders as a Tauri-style capsule
                // pill at `(zone.x, zone.y)` consuming the Wave B token SSoT in
                // `zone_pill_geometry`.
                //
                // #4 / R1 — a stack anchor's HOVER affordance is the bloom, NOT a
                // panel expand (Tauri `StackWrapper.tsx` has no hover-to-expand
                // state). So an anchor's body is visible only when it is explicitly
                // selected (a focused member) or being dragged — never on mere
                // hover — so the collapsed pill + bloom can co-exist without the
                // panel popping underneath them.
                // (Shared SSoT — `zone_on_top` above keys off the SAME predicate.)
                let pill_body_visible = app.zone_pill_body_visible(zone);
                // Wave G2 — morphing capsule. When the hover transition is
                // in-flight for this zone, paint an intermediate rounded-rect
                // instead of snapping between collapsed pill and expanded body.
                // Stack anchors don't run the pill↔panel morph (they toggle between
                // the compact pill and the focused-member panel without it).
                let pill_anim_active = app.zone_pill_morph_in_flight(zone);
                if pill_anim_active {
                    let count = zone.items.len();
                    let pill_layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
                    let expanded_rect = bento_nano_style::Rect {
                        x: zone.x as f32,
                        y: zone.y as f32,
                        width: zone.w as f32,
                        height: zone.h as f32,
                    };
                    let raw = app.zone_pill_anim_progress.get();
                    // The shared monotonic `current_morph_rect` is the single
                    // structural visual state, so paint == hit
                    // geometry (effective_zone_chrome_rect / effective_zone_hit_rect
                    // call the same helper). `draw_zone_pill_morph` re-derives the
                    // rect from the same `morph` via `morph_pill_to_rect`, so the
                    // returned rect here is discarded but stays bit-identical.
                    let (morph, _morph_rect) = zone_pill_geometry::current_morph_rect(
                        pill_layout.rect,
                        expanded_rect,
                        app.zone_pill_anim_from_morph.get(),
                        raw,
                        app.zone_pill_anim_expanding.get(),
                    );
                    // V21-C9 — still sample the V-8 PillHover channel at the
                    // morph boundary, but keep the collapsed endpoint at the
                    // exact Tauri `surface_zen` token. Tauri has no hover
                    // background rule for `.bento-zone--zen`; hover feedback is
                    // shape/shadow/transform-specific, not a base-fill brighten.
                    let hover_t = {
                        let anim = app.pill_animator.borrow();
                        anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms)
                    };
                    // One visual state: the same morph drives surface geometry,
                    // identity placement, hit/chrome bounds, and expanded-only
                    // content. A second delayed alpha timeline made the shell
                    // arrive before its contents and read as a detached layer.
                    self.draw_zone_pill_morph(
                        app,
                        zone,
                        &pill_layout,
                        expanded_rect,
                        morph,
                        hover_t,
                        pal,
                        &item_chrome,
                        effect,
                    )?;
                    continue;
                }
                if !pill_body_visible {
                    if let Some(member_ids) = app.zones.stack_member_ids(zone.id) {
                        let layout = zone_pill_geometry::stack_capsule_layout_for_zone(
                            zone,
                            member_ids.len(),
                        );
                        let (hover_t, press_t, emerge_progress) = {
                            let anim = app.pill_animator.borrow();
                            (
                                anim.sample(zone.id, animator::AnimChannel::PillHover, anim_now_ms),
                                anim.sample(zone.id, animator::AnimChannel::PillPress, anim_now_ms),
                                1.0 - anim.sample(
                                    zone.id,
                                    animator::AnimChannel::StackEmerge,
                                    anim_now_ms,
                                ),
                            )
                        };
                        self.draw_stack_capsule(
                            app,
                            zone,
                            member_ids.as_slice(),
                            &layout,
                            hover_t,
                            press_t,
                            emerge_progress,
                            pal,
                            shadow_tauri.zen,
                            effect,
                        )?;
                        continue;
                    }
                    let count = collapsed_pill_display_count(app, zone);
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
                        count,
                        hover_t,
                        press_t,
                        zone_drag_visual_opacity(app, zone.id),
                        anim_now_ms,
                        pal,
                        shadow_tauri.zen,
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
                // #4 (2026-06-02) — a focused stack member (incl. the anchor) now
                // renders as the NORMAL expanded panel, so the shadow is no longer
                // suppressed for anchors (the bespoke anchor halo + double-shadow
                // that this guard avoided double-stamping was removed below).
                let expanded_layout = expanded_zone_grid::expanded_zone_layout(zone);
                {
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
                // #4 (2026-06-02) — the per-anchor wrapper halo + double drop-shadow
                // were REMOVED: they have no Tauri reference counterpart and were
                // part of the bug-screenshot pile-up. A focused stack member now
                // renders as the NORMAL expanded panel (the `!zone.is_stack_anchor()`
                // shadow above), and a collapsed anchor renders as the compact pill.
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
                // Frosted-backdrop (2026-06-02 real-blur inversion) — the settled
                // expanded panel surface is real acrylic: [blurred+saturated desktop
                // clipped to the panel rect] + [ONE tint] (`surface_expanded` 82%
                // idle / `accent@0.92` active), matching Tauri's panel chrome over
                // `backdrop-filter: blur(24px) saturate(1.7)`. The idle tint dropped
                // from `surface_dialog` 0.92 → `surface_expanded` 0.82 (P1.3): at
                // 0.92 the blur was masked, the dominant "完全不一样" delta. Frosting
                // under the active-drag accent tint is intentional (Tauri's accent
                // panels also sit over the blur). Degrades to the flat tint when no
                // backdrop. The M6b shadow stack + accent edge are unchanged.
                self.fill_frosted_rect(rect, fill, radius)?;
                // P2.2 — the 1px white-12% panel hairline (Tauri
                // `.bento-zone--expanded { border: 1px solid rgba(255,255,255,0.12) }`
                // = `--border-expanded`) that nano never painted. Stroked AFTER the
                // frosted fill and BEFORE the accent top-edge below so the 2px accent
                // bar layers over the hairline (CSS border-top paints over the box
                // border). `stroke_rounded_rect` short-circuits on `color.a <= 0.0`.
                self.stroke_rounded_rect(rect, pal.border_expanded, radius, 1.0)?;
                if let Some(accent) = zone.accent_color.as_deref().and_then(parse_hex_color) {
                    self.draw_expanded_panel_accent_edge(rect, radius, accent)?;
                }
                let body_visible = pill_body_visible;
                self.draw_expanded_panel_header(app, zone, &expanded_layout, pal, 1.0, true)?;
                let zone_search_active = zone_search_target == Some(zone.id);
                let zone_search_reveal = if zone_search_active {
                    app.zone_search_animation_progress_at(anim_now_ms)
                } else {
                    0.0
                };
                if zone_search_active {
                    self.draw_inline_zone_search(app, rect, zone_search_query.as_str())?;
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
                // #4 (2026-06-02) — the "Stack ×N" badge + "Peek: <member>" sub-row
                // were REMOVED. They have no Tauri reference counterpart and were
                // part of the bug-screenshot pile-up. Stack membership is now
                // conveyed by the collapsed pill's count badge; a focused member
                // uses the normal expanded panel.
                if !body_visible {
                    continue;
                }
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
                    self.fill_rounded_rect(
                        live_rect,
                        zone_live_folder_fill,
                        zone_chrome.live_badge_radius,
                    )?;
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
                let item_top_offset = search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX * zone_search_reveal;
                let item_scroll_max = if zone_search_active {
                    highlight_overlay::item_flow_max_scroll(
                        zone,
                        item_top_offset,
                        zone.items
                            .iter()
                            .filter(|item| {
                                search_bar::zone_item_matches_query(
                                    item.name.as_ref(),
                                    zone_search_query.as_str(),
                                )
                            })
                            .map(|item| item.is_wide),
                    )
                } else {
                    highlight_overlay::item_flow_max_scroll(
                        zone,
                        item_top_offset,
                        zone.items.iter().map(|item| item.is_wide),
                    )
                };
                let item_scroll = app.zone_content_scroll_offset(zone.id).min(item_scroll_max);
                let content_clip = highlight_overlay::item_content_clip_rect(zone, item_top_offset);
                self.push_clip(content_clip)?;
                let content_result = (|| -> Result<(), RenderError> {
                    let item_label_group_px = {
                        let mut label_flow_slot = 0;
                        item_label_group_font_size(zone.items.iter().filter_map(|item| {
                            if zone_search_active
                                && !search_bar::zone_item_matches_query(
                                    item.name.as_ref(),
                                    zone_search_query.as_str(),
                                )
                            {
                                return None;
                            }
                            let card_rect = if zone_search_active {
                                let (card, next_slot) =
                                    highlight_overlay::item_card_rect_for_flow_slot_scrolled(
                                        zone,
                                        label_flow_slot,
                                        item.is_wide,
                                        item_top_offset,
                                        item_scroll,
                                    );
                                label_flow_slot = next_slot;
                                card
                            } else {
                                highlight_overlay::item_card_rect_for_item_scrolled(
                                    zone,
                                    item,
                                    item_scroll,
                                )
                            };
                            (card_rect.width > 0.0).then_some((
                                item_label_visible_name(item.name.as_ref()),
                                (card_rect.width - 8.0).max(0.0),
                            ))
                        }))
                    };
                    let mut search_flow_slot = 0;
                    let mut visible_item_count = 0usize;
                    for item in &zone.items {
                        if zone_search_active
                            && !search_bar::zone_item_matches_query(
                                item.name.as_ref(),
                                zone_search_query.as_str(),
                            )
                        {
                            continue;
                        }
                        visible_item_count += 1;
                        let card_rect = if zone_search_active {
                            let (card, next_slot) =
                                highlight_overlay::item_card_rect_for_flow_slot_scrolled(
                                    zone,
                                    search_flow_slot,
                                    item.is_wide,
                                    item_top_offset,
                                    item_scroll,
                                );
                            search_flow_slot = next_slot;
                            card
                        } else {
                            highlight_overlay::item_card_rect_for_item_scrolled(
                                zone,
                                item,
                                item_scroll,
                            )
                        };
                        if card_rect.width <= 0.0
                            || card_rect.bottom() <= content_clip.y
                            || card_rect.y >= content_clip.bottom()
                        {
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
                        //
                        // M3-A3 — Tauri removes the entire `:hover` rule on a
                        // `aria-disabled` (missing-file) card, and a drag-source card
                        // shows its muted placeholder bg, never the hover chrome. So we
                        // zero `hover_t` for both: only a present, non-dragged card
                        // lifts / lerps its bg-border-shadow.
                        let card_key = (zone.id, item.id);
                        let item_hover = app.item_hover.get();
                        let (hover_raw, press_t) = if is_dragged_source {
                            (0.0, 0.0)
                        } else {
                            item_hover.sample(card_key, anim_now_ms)
                        };
                        let hover_t = if is_dragged_source || item.file_missing {
                            0.0
                        } else {
                            hover_raw
                        };
                        let item_scale = if is_dragged_source {
                            1.0
                        } else {
                            item_card::card_scale_for(hover_raw, press_t)
                        };
                        // FIX 1 — drop the translateY lift only while the pointer is
                        // actively held (Tauri `:active` scale-only override). On
                        // release the lift returns while the press scale ramps out.
                        let press_held = !is_dragged_source && item_hover.press_held(card_key);
                        self.draw_item_card(
                            item,
                            card_rect,
                            item_fill,
                            &item_chrome,
                            hover_t,
                            press_held,
                            item_scale,
                            item_label_group_px,
                            1.0,
                        )?;
                    }
                    if zone_search_active && visible_item_count == 0 {
                        self.draw_text_no_wrap_with_style(
                            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SEARCH_EMPTY),
                            bento_nano_style::Rect {
                                x: rect.x + expanded_zone_grid::HEADER_INSET_X,
                                y: rect.y + item_grid::ITEM_GRID_TOP_OFFSET_PX + item_top_offset,
                                width: (rect.width - expanded_zone_grid::HEADER_INSET_X * 2.0)
                                    .max(0.0),
                                height: 28.0,
                            },
                            pal.text_muted,
                            12.0,
                            400,
                            1.4,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Center,
                                v: dwrite::VAlign::Center,
                            },
                        )?;
                    }
                    if Some(zone.id) == drag_target_id {
                        if let Some(preview) = drop_preview_rect_for_zone(
                            zone,
                            item_drag,
                            dragged_item_wide,
                            item_scroll,
                            item_top_offset,
                        ) {
                            // Drag preview is a target affordance, not card chrome. Paint it
                            // after resident cards so occupied cells cannot cover the target core,
                            // but before the floating ghost so the dragged item remains topmost.
                            self.fill_rounded_rect(
                                preview,
                                drop_preview_fill,
                                item_chrome.card_radius,
                            )?;
                            let core = inset_rect(preview, 4.0);
                            self.fill_rounded_rect(
                                core,
                                drop_preview_core,
                                zone_chrome.drop_preview_core_radius,
                            )?;
                        }
                    }
                    Ok(())
                })();
                let pop_result = self.pop_clip();
                content_result?;
                pop_result?;

                // Tauri uses a transparent track and a narrow content thumb.
                // Keep it subtle but visible so clipped rows advertise that
                // the panel can be scrolled instead of looking truncated.
                if item_scroll_max > 0.0 && content_clip.height > 8.0 {
                    let track = bento_nano_style::Rect {
                        x: rect.right() - 5.0,
                        y: content_clip.y + 4.0,
                        width: 3.0,
                        height: (content_clip.height - 8.0).max(0.0),
                    };
                    let viewport_ratio =
                        content_clip.height / (content_clip.height + item_scroll_max);
                    let thumb_height = (track.height * viewport_ratio)
                        .clamp(18.0_f32.min(track.height), track.height);
                    let travel = (track.height - thumb_height).max(0.0);
                    let progress = item_scroll / item_scroll_max;
                    self.fill_rounded_rect(
                        bento_nano_style::Rect {
                            x: track.x,
                            y: track.y + travel * progress,
                            width: track.width,
                            height: thumb_height,
                        },
                        with_alpha(pal.text_primary, 0.22),
                        bento_nano_style::BorderRadius::all(2.0),
                    )?;
                }
                // M2 E-01 (2026-05-29) — the 16×16 sub-zone footer thumbnail
                // strip was DELETED. Tauri's `BentoPanel` renders header + grid
                // only with no footer node; the strip was an additive nano
                // divergence visible only on stack anchors. Removed for 1:1.
            }
        }
        // Z-order (2026-06-02) — the hover-bloom is drawn AFTER both layers, so
        // it stays above every pill AND every panel. It is a hover affordance on
        // a COLLAPSED stack anchor and is gated to frames where no zone is
        // expanded/selected (so it never co-renders with a panel) — keeping it
        // last preserves the current visual intent (top of the whole zone stack)
        // and matches the hit side, where `push_stack_overlay_rects` is pushed
        // before the per-zone rects so the bloom petals win the hit-test.
        // #4 / R1 (2026-06-02) — the hover-bloom is a real Tauri feature
        // (`StackWrapper.tsx` hover-bloom), so it is GATED, not deleted. It
        // fans out ONLY when (a) the cursor hovers a stack anchor, (b) the
        // explicit management tray is closed (`stack_tray` is None — they are
        // mutually exclusive surfaces), and (c) no zone is expanded/selected
        // (so it can never co-render with an expanded panel). Step 5 separately
        // ensures the bloom trigger only arms for actual stack anchors.
        // `selected_zone.is_none()` means no member is focused (no expanded
        // anchor panel), so the bloom can never co-render with the focused-
        // member panel. The anchor's own hover does NOT expand it (see the
        // `pill_body_visible` anchor rule above), so the collapsed pill + bloom
        // are the only surfaces shown while hovering.
        let bloom_allowed = stack_surface_allows_bloom(app);
        // `stack_bloom_anchor` is the sole structural owner. A plain
        // `hovered_zone` is deliberately insufficient: pointer drop creates
        // the model relation first and then explicitly arms this owner, while
        // context-menu stacking stays collapsed until a real hover/click.
        if let Some(anchor_id) = app.stack_bloom_anchor.get().filter(|_| bloom_allowed) {
            if let Some(anchor) = app.zones.get(anchor_id) {
                if let Some(member_ids) = app.zones.stack_member_ids(anchor.id) {
                    let frames = if app.stack_bloom_leaving.get()
                        && app.stack_bloom_anchor.get() == Some(anchor.id)
                    {
                        stack_tray::stack_bloom_exit_frames_at(
                            app.viewport,
                            anchor,
                            member_ids.len(),
                            app.stack_bloom_progress.get(),
                        )
                    } else {
                        let reveal_progress = if app.stack_bloom_anchor.get() == Some(anchor.id) {
                            app.stack_bloom_progress.get()
                        } else {
                            1.0
                        };
                        stack_tray::stack_bloom_frames_at(
                            app.viewport,
                            anchor,
                            member_ids.len(),
                            reveal_progress,
                        )
                    };
                    let petal_size = stack_tray::stack_bloom_petal_size(member_ids.len());
                    let bloom_interaction = app.stack_bloom_interaction.get();
                    let overflow_count = stack_tray::stack_bloom_overflow_count(member_ids.len());
                    let member_frame_count =
                        frames.len().saturating_sub(usize::from(overflow_count > 0));
                    for (member_id, frame) in member_ids
                        .iter()
                        .copied()
                        .take(member_frame_count)
                        .zip(frames.iter().copied().take(member_frame_count))
                    {
                        let Some(member) = app.zones.get(member_id) else {
                            continue;
                        };
                        let active = bloom_interaction.active_member == Some(member_id);
                        let active_t = if active {
                            stack_bloom_active_transition_t(
                                anim_now_ms,
                                bloom_interaction.active_member_started_ms,
                            )
                        } else {
                            0.0
                        };
                        let active_scale = 1.0 + (STACK_BLOOM_ACTIVE_SCALE - 1.0) * active_t;
                        let petal_rect = animator::scale_rect_centered(frame.rect, active_scale);
                        if frame.connector.width > 0.5 && frame.connector.height > 0.5 {
                            self.fill_rounded_rect(
                                frame.connector,
                                with_alpha(palette.accent, 0.16 * frame.alpha),
                                zone_chrome.bloom_connector_radius,
                            )?;
                        }
                        let accent = member
                            .accent_color
                            .as_deref()
                            .and_then(parse_hex_color)
                            .unwrap_or(pal.accent_blue);
                        // W14 — do not fake CSS blur with a second opaque
                        // offset tile. That hard duplicate is the black slab
                        // visible around every Bloom petal; ordinary blurred
                        // layers follow the shared W13-B suppression contract.
                        if active_t > 0.0 {
                            let (pulse_spread, pulse_alpha) = stack_bloom_active_pulse(
                                anim_now_ms,
                                bloom_interaction.active_member_started_ms,
                                member_ids.len() > 8,
                            );
                            let pulse_rect = bento_nano_style::Rect {
                                x: petal_rect.x - pulse_spread,
                                y: petal_rect.y - pulse_spread,
                                width: petal_rect.width + pulse_spread * 2.0,
                                height: petal_rect.height + pulse_spread * 2.0,
                            };
                            self.fill_rounded_rect(
                                pulse_rect,
                                with_alpha(accent, pulse_alpha * active_t * frame.alpha),
                                BorderRadius::all(16.0 * active_scale + pulse_spread),
                            )?;
                            let ring_spread = 1.5;
                            let ring_rect = bento_nano_style::Rect {
                                x: petal_rect.x - ring_spread,
                                y: petal_rect.y - ring_spread,
                                width: petal_rect.width + ring_spread * 2.0,
                                height: petal_rect.height + ring_spread * 2.0,
                            };
                            self.fill_rounded_rect(
                                ring_rect,
                                with_alpha(accent, active_t * frame.alpha),
                                BorderRadius::all(16.0 * active_scale + ring_spread),
                            )?;
                        }
                        // Match Tauri's fixed Bloom cards: both the desktop
                        // backdrop and the theme tint participate in the same
                        // entry/exit opacity. Painting only a translucent tint
                        // leaves Explorer labels razor-sharp through the petal;
                        // fading only the tint leaves a hard backdrop slab.
                        self.fill_frosted_rect_with_group_opacity(
                            petal_rect,
                            pal.surface_expanded,
                            zone_chrome.bloom_petal_radius,
                            frame.alpha,
                        )?;
                        self.fill_rounded_rect_linear_gradient(
                            petal_rect,
                            with_alpha(bento_nano_style::Color::WHITE, 0.14 * frame.alpha),
                            with_alpha(bento_nano_style::Color::WHITE, 0.04 * frame.alpha),
                            zone_chrome.bloom_petal_radius,
                            stack_capsule_sheen_gradient_props(petal_rect),
                        )?;
                        self.stroke_rounded_rect(
                            petal_rect,
                            lerp_color(
                                with_alpha(bento_nano_style::Color::WHITE, 0.22 * frame.alpha),
                                with_alpha(accent, frame.alpha),
                                active_t,
                            ),
                            zone_chrome.bloom_border_radius,
                            1.0 + 0.5 * active_t,
                        )?;
                        let content_scale = frame.scale * active_scale;
                        let icon_side = (petal_size.icon_size * content_scale).clamp(
                            18.0,
                            (petal_rect.width.min(petal_rect.height) - 16.0).max(18.0),
                        );
                        let content = stack_tray::stack_bloom_petal_content_layout(
                            petal_rect,
                            icon_side,
                            content_scale,
                        );
                        let icon_rect = content.icon_rect;
                        let icon_radius =
                            BorderRadius::all(icon_rect.width.min(icon_rect.height) * 0.5);
                        self.fill_rounded_rect(
                            icon_rect,
                            with_alpha(accent, (0.78 + 0.22 * active_t) * frame.alpha),
                            icon_radius,
                        )?;
                        self.stroke_rounded_rect(
                            icon_rect,
                            lerp_color(
                                with_alpha(bento_nano_style::Color::WHITE, 0.14 * frame.alpha),
                                with_alpha(accent, 0.60 * frame.alpha),
                                active_t,
                            ),
                            icon_radius,
                            1.0,
                        )?;
                        self.draw_icon_glyph(
                            member.icon.as_ref(),
                            centered_square_rect(icon_rect, (icon_side * 0.60).max(12.0)),
                            with_alpha(pal.text_primary, frame.alpha),
                        )?;
                        self.draw_stack_bloom_petal_name(
                            member.display_title(),
                            content.title_rect,
                            with_alpha(pal.text_primary, 0.92 * frame.alpha),
                        )?;
                    }
                    if overflow_count > 0 {
                        if let Some(frame) = frames.last().copied() {
                            let overflow_rect = frame.rect;
                            self.fill_frosted_rect_with_group_opacity(
                                overflow_rect,
                                pal.surface_zen,
                                zone_chrome.bloom_petal_radius,
                                frame.alpha,
                            )?;
                            self.fill_rounded_rect_linear_gradient(
                                overflow_rect,
                                with_alpha(bento_nano_style::Color::WHITE, 0.06 * frame.alpha),
                                with_alpha(bento_nano_style::Color::WHITE, 0.02 * frame.alpha),
                                zone_chrome.bloom_petal_radius,
                                stack_capsule_sheen_gradient_props(overflow_rect),
                            )?;
                            self.stroke_rounded_rect(
                                overflow_rect,
                                with_alpha(bento_nano_style::Color::WHITE, 0.12 * frame.alpha),
                                zone_chrome.bloom_border_radius,
                                1.0,
                            )?;
                            let count = format_small_count(overflow_count);
                            let token_width = 44.0 * frame.scale;
                            let plus_width = 14.0 * frame.scale;
                            let token_rect = bento_nano_style::Rect {
                                x: overflow_rect.x + (overflow_rect.width - token_width) * 0.5,
                                y: overflow_rect.y,
                                width: token_width,
                                height: overflow_rect.height,
                            };
                            self.draw_text_no_wrap_with_style(
                                "+",
                                bento_nano_style::Rect {
                                    width: plus_width,
                                    ..token_rect
                                },
                                with_alpha(pal.text_primary, 0.70 * frame.alpha),
                                18.0 * frame.scale,
                                700,
                                1.2,
                                dwrite::TextAlign {
                                    h: dwrite::HAlign::Center,
                                    v: dwrite::VAlign::Center,
                                },
                            )?;
                            self.draw_text_no_wrap_with_style(
                                count.as_str(),
                                bento_nano_style::Rect {
                                    x: token_rect.x + plus_width,
                                    width: token_rect.width - plus_width,
                                    ..token_rect
                                },
                                with_alpha(pal.text_primary, 0.70 * frame.alpha),
                                18.0 * frame.scale,
                                700,
                                1.2,
                                dwrite::TextAlign {
                                    h: dwrite::HAlign::Center,
                                    v: dwrite::VAlign::Center,
                                },
                            )?;
                        }
                    }
                }
            }
        }
        if let Some(drag) = item_drag {
            if let Some((zone, item)) = source_drag_item(app, drag) {
                let source_rect = item_card_rect_for_item(zone, item);
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
                    &item_chrome,
                    // M3-A2/A3 — the floating drag ghost is not a hover target;
                    // it keeps identity scale + zero hover_t (no lift / bg-border
                    // -shadow lerp; the ghost has its own lift/shadow treatment)
                    // so hover/press chrome stays on the live grid.
                    0.0,
                    false,
                    1.0,
                    item_label_font_size_for_width(
                        item_label_visible_name(item.name.as_ref()),
                        (ghost_rect.width - 8.0).max(0.0),
                    ),
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

    /// Paint the stack-specific collapsed capsule from Tauri `StackCapsule.tsx`.
    ///
    /// This is intentionally separate from `draw_zone_pill`: stack anchors have
    /// their own 220x52 grid with overlapped member peeks, a top-member icon
    /// bubble, title, and member-count badge. The anchor zone remains the
    /// command/hit root; the visual top zone follows Tauri's sorted stack order.
    #[allow(clippy::too_many_arguments)]
    fn draw_stack_capsule(
        &mut self,
        app: &AppState,
        anchor: &Zone,
        member_ids: &[ZoneId],
        layout: &StackCapsuleLayout,
        hover_t: f32,
        press_t: f32,
        emerge_progress: f32,
        pal: bento_nano_style::tokens::PaletteTauri,
        shadow_zen: bento_nano_style::ShadowStack,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        let hover_t = hover_t.clamp(0.0, 1.0);
        let is_locked = stack_capsule_is_locked(app, anchor, member_ids);
        let has_preview = stack_capsule_has_preview(app, anchor.id);
        let bloom = if is_locked {
            stack_capsule_bloom_visual(0.0, member_ids.len(), false)
        } else {
            stack_capsule_bloom_visual_for_app(app, anchor.id, member_ids.len())
        };
        let emerge = stack_capsule_presented_emerge_visual(emerge_progress);
        let visual_scale = bloom.scale * emerge.scale;
        let capsule_opacity = bloom.opacity
            * emerge.opacity
            * stack_capsule_locked_opacity(is_locked)
            * zone_drag_visual_opacity(app, anchor.id);
        let visual_dy = stack_capsule_hover_translate_y(hover_t) * (1.0 - bloom.recede_t);
        let base_rect = translate_rect(layout.rect, 0.0, visual_dy);
        let visual_rect = animator::scale_rect_centered(base_rect, visual_scale);
        let visual_radius = scale_border_radius(layout.radius, visual_scale);
        let child_rect = |rect| {
            scale_rect_about_center(
                translate_rect(rect, 0.0, visual_dy),
                base_rect,
                visual_scale,
            )
        };
        if let bento_nano_style::tokens::EffectTauri::Neon(n) = effect {
            self.draw_neon_glow(visual_rect, n.collapsed, visual_radius)?;
        }
        self.draw_shadow_stack(
            visual_rect,
            scale_shadow_stack(
                fade_shadow_stack(
                    stack_capsule_visual_shadow_stack(
                        shadow_zen,
                        hover_t,
                        bloom.recede_t,
                        has_preview,
                    ),
                    capsule_opacity,
                ),
                visual_scale,
            ),
            visual_radius,
        )?;
        let surface_color = collapsed_zen_surface_color(pal, hover_t);
        self.fill_frosted_rect_with_group_opacity(
            visual_rect,
            surface_color,
            visual_radius,
            capsule_opacity,
        )?;
        let (sheen_start, sheen_end) = stack_capsule_glass_sheen_colors();
        self.fill_rounded_rect_linear_gradient(
            visual_rect,
            fade_color(sheen_start, capsule_opacity),
            fade_color(sheen_end, capsule_opacity),
            visual_radius,
            stack_capsule_sheen_gradient_props(visual_rect),
        )?;
        self.stroke_rounded_rect(
            visual_rect,
            fade_color(
                stack_capsule_bloom_border_color(pal, hover_t, bloom.recede_t),
                capsule_opacity,
            ),
            visual_radius,
            1.0,
        )?;

        let chip_fill = fade_color(with_alpha(pal.text_primary, 0.08), capsule_opacity);
        let chip_border = fade_color(with_alpha(pal.text_primary, 0.10), capsule_opacity);
        let content_color = fade_color(pal.text_primary, capsule_opacity);
        let badge_chrome = stack_capsule_badge_chrome(pal, is_locked);
        let peek_start = member_ids.len().saturating_sub(layout.peek_visible_count);
        let mut slot = 0;
        while slot < layout.peek_visible_count {
            let peek_rect = child_rect(layout.peek_icons[slot]);
            let peek_radius = scale_border_radius(layout.peek_radius, visual_scale);
            self.fill_rounded_rect(peek_rect, chip_fill, peek_radius)?;
            self.stroke_rounded_rect(peek_rect, chip_border, peek_radius, 1.0)?;
            if let Some(member) = member_ids
                .get(peek_start + slot)
                .and_then(|member_id| app.zones.get(*member_id))
            {
                self.draw_icon_glyph(
                    member.icon.as_ref(),
                    centered_square_rect(peek_rect, 12.0 * visual_scale),
                    content_color,
                )?;
            }
            slot += 1;
        }

        let top_zone = member_ids
            .last()
            .and_then(|member_id| app.zones.get(*member_id))
            .unwrap_or(anchor);
        let icon_bubble = child_rect(layout.icon_bubble);
        let icon_glyph = child_rect(layout.icon_glyph);
        let badge = child_rect(layout.badge);
        let mut label_layout = layout.label;
        let preview_label =
            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STACK_PREVIEW_ACTIVE);
        let preview_indicator_layout =
            if stack_capsule_show_preview_indicator(has_preview, bloom.recede_t) {
                let width = stack_capsule_preview_indicator_width(preview_label);
                let x = (layout.badge.x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - width)
                    .max(layout.label.x);
                label_layout.width =
                    (x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - label_layout.x).max(0.0);
                Some(Rect {
                    x,
                    y: layout.rect.y
                        + (layout.rect.height - STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT) * 0.5,
                    width: (layout.badge.x - zone_pill_geometry::STACK_CAPSULE_GAP_PX - x).max(0.0),
                    height: STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT,
                })
            } else {
                None
            };
        let label_text = translate_rect(label_layout, 0.0, visual_dy);
        let badge_text = translate_rect(layout.badge, 0.0, visual_dy);
        let text_transform =
            stack_capsule_bloom_text_transform(self.base_scale, base_rect, visual_scale);
        self.fill_rounded_rect(
            icon_bubble,
            chip_fill,
            scale_border_radius(layout.icon_radius, visual_scale),
        )?;
        self.draw_icon_glyph(top_zone.icon.as_ref(), icon_glyph, content_color)?;
        self.draw_stack_capsule_title_shrink_to_fit_transformed(
            top_zone.display_title(),
            label_text,
            content_color,
            text_transform,
        )?;
        if let Some(indicator) = preview_indicator_layout {
            let indicator = child_rect(indicator);
            self.fill_rounded_rect(
                indicator,
                fade_color(STACK_CAPSULE_PREVIEW_INDICATOR_FILL, capsule_opacity),
                BorderRadius::all(indicator.height * 0.5),
            )?;
            self.draw_text_no_wrap_with_style_transformed(
                preview_label,
                indicator,
                fade_color(STACK_CAPSULE_PREVIEW_INDICATOR_TEXT, capsule_opacity),
                STACK_CAPSULE_PREVIEW_INDICATOR_FONT_PX,
                STACK_CAPSULE_PREVIEW_INDICATOR_FONT_WEIGHT,
                1.2,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
                text_transform,
            )?;
        }

        self.fill_rounded_rect(
            badge,
            fade_color(badge_chrome.fill, capsule_opacity),
            scale_border_radius(layout.badge_radius, visual_scale),
        )?;
        let count_str = format_small_count(member_ids.len());
        self.draw_text_no_wrap_with_style_transformed(
            count_str.as_str(),
            badge_text,
            fade_color(badge_chrome.text, capsule_opacity),
            zone_pill_geometry::STACK_CAPSULE_BADGE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_BADGE_FONT_WEIGHT,
            1.2,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
            text_transform,
        )?;
        let _ = press_t;
        Ok(())
    }

    /// Paint the complete expanded PanelHeader layer at `opacity`.
    ///
    /// The settled panel and the in-flight Bento layer share this exact path so
    /// the final spring frame cannot pop from a title-only proxy to the real
    /// icon/badge/search/close chrome. Tauri keeps the Bento layer mounted and
    /// changes only its opacity; Nano mirrors that contract here.
    fn draw_expanded_panel_header(
        &mut self,
        app: &AppState,
        zone: &Zone,
        layout: &expanded_zone_grid::ExpandedZoneLayout,
        pal: bento_nano_style::tokens::PaletteTauri,
        opacity: f32,
        draw_identity: bool,
    ) -> Result<(), RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }

        if draw_identity {
            let pill = zone_pill_geometry::pill_layout_for_zone(zone, zone.items.len());
            let identity = morph_zen_content_to_header(pill, layout, 1.0);
            self.draw_zone_pill_content(zone, &identity, zone.items.len(), opacity, pal)?;
        }

        let search_btn = layout.header_search_btn;
        let close_btn = layout.header_close_btn;
        let search_chrome = panel_header_button_chrome(
            pal,
            PanelHeaderButtonKind::Search,
            app.is_panel_header_button_hovered(zone.id, PanelHeaderButtonKind::Search),
        );
        let close_chrome = panel_header_button_chrome(
            pal,
            PanelHeaderButtonKind::Close,
            app.is_panel_header_button_hovered(zone.id, PanelHeaderButtonKind::Close),
        );
        let button_radius =
            bento_nano_style::BorderRadius::all(expanded_zone_grid::HEADER_BTN_RADIUS);
        if let Some(background) = search_chrome.background {
            self.fill_rounded_rect(search_btn, fade_color(background, opacity), button_radius)?;
        }
        if let Some(background) = close_chrome.background {
            self.fill_rounded_rect(close_btn, fade_color(background, opacity), button_radius)?;
        }
        let glyph_size = expanded_zone_grid::HEADER_BTN_GLYPH_SIZE;
        let glyph_inset = |button: bento_nano_style::Rect| bento_nano_style::Rect {
            x: button.x + (button.width - glyph_size) * 0.5,
            y: button.y + (button.height - glyph_size) * 0.5,
            width: glyph_size,
            height: glyph_size,
        };
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            glyph_inset(search_btn),
            fade_color(search_chrome.glyph, opacity),
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            glyph_inset(close_btn),
            fade_color(close_chrome.glyph, opacity),
        )?;
        self.fill_rounded_rect(
            layout.divider,
            with_alpha(bento_nano_style::Color::WHITE, 0.05 * opacity),
            bento_nano_style::BorderRadius::ZERO,
        )?;
        Ok(())
    }

    /// Paint only the collapsed Zen layer's icon/title/count row.
    /// Surface, shadow, and border remain owned by the outer pill/morph paths.
    fn draw_zone_pill_content(
        &mut self,
        zone: &Zone,
        layout: &ZonePillLayout,
        display_count: usize,
        opacity: f32,
        pal: bento_nano_style::tokens::PaletteTauri,
    ) -> Result<(), RenderError> {
        use crate::business::zen_capsule::CapsuleSize;

        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }
        let size = CapsuleSize::parse(zone.capsule_size.as_ref());
        let display_title = zone.display_title();
        self.draw_icon_glyph(
            zone.icon.as_ref(),
            layout.icon,
            fade_color(pal.text_primary, opacity),
        )?;
        let uses_visible_glyph_content_metrics =
            zone_pill_geometry::pill_uses_visible_glyph_content_metrics(
                size,
                zone.icon.as_ref(),
                display_title,
            );
        let title_font_px = zone_pill_geometry::pill_title_font_px_for_text(
            size,
            uses_visible_glyph_content_metrics,
            display_title,
        );
        let title_tracking_px = zone_pill_geometry::pill_title_tracking_px_for(
            size,
            uses_visible_glyph_content_metrics,
        );
        let title_color = with_alpha(
            pal.text_primary,
            zone_pill_geometry::pill_title_alpha_for(size, uses_visible_glyph_content_metrics),
        );
        let title_rect = bento_nano_style::Rect {
            x: layout.label.x,
            y: layout.rect.y,
            width: layout.label.width,
            height: layout.rect.height,
        };
        self.draw_pill_title_ellipsis(
            display_title,
            title_rect,
            fade_color(title_color, opacity),
            title_font_px,
            title_tracking_px,
        )?;

        let badge_fill = tauri_badge_fill(zone.accent_color.as_deref(), pal.badge_bg);
        self.fill_rounded_rect(
            layout.badge,
            fade_color(badge_fill, opacity),
            layout.badge_radius,
        )?;
        let count_str = format_small_count(display_count);
        let (badge_pad_x, _) = size.badge_padding_xy();
        let badge_text_rect = bento_nano_style::Rect {
            x: layout.badge.x + badge_pad_x,
            // DirectWrite centers the line box, while rounded count badges are
            // judged by the visible digit ink. A half-DIP optical nudge keeps
            // the rasterized numeral centered at both 100% and 150% DPI.
            y: layout.badge.y + 0.5,
            width: (layout.badge.width - badge_pad_x * 2.0).max(0.0),
            height: layout.badge.height,
        };
        self.draw_text_no_wrap_with_style(
            count_str.as_str(),
            badge_text_rect,
            fade_color(pal.text_primary, opacity),
            size.badge_font_px(),
            size.badge_font_weight(),
            // DWrite vertically centres the uniform line box, not the visible
            // digit ink. A 1.4 line box lifts the 10/11-DIP numeral by ~2 DIP
            // inside this fixed badge; a tight line box restores optical centre.
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        Ok(())
    }

    /// Wave C (05-20 visual parity) — collapsed zone pill render path.
    /// Tauri shows ordinary zones as a rounded capsule (icon + name + count
    /// badge) by default. The live `pal: PaletteTauri` is threaded in from
    /// `draw_zones` so the pill re-skins with the active theme. The paint
    /// inputs are genuinely distinct, so the arity is allowed rather than
    /// bundled.
    #[allow(clippy::too_many_arguments)]
    fn draw_zone_pill(
        &mut self,
        zone: &Zone,
        layout: &ZonePillLayout,
        display_count: usize,
        hover_t: f32,
        press_t: f32,
        opacity: f32,
        anim_now_ms: u32,
        pal: bento_nano_style::tokens::PaletteTauri,
        shadow_zen: bento_nano_style::ShadowStack,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — the live theme palette is passed in by `draw_zones` (bound
        // once per frame). Read `pal.X` instead of the static `PALETTE_DARK`
        // so the collapsed pill re-skins with the active theme.
        // Frosted-backdrop (2026-06-01) — `ACRYLIC_FALLBACK` is no longer used
        // here: the collapsed pill's old `ACRYLIC_FALLBACK` + `surface_zen`
        // double layer is replaced by one `fill_frosted_rect` (blur + single
        // tint), so the import is dropped to stay warning-clean.
        use crate::business::zen_capsule::{CapsuleShape, CapsuleSize};
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }
        // G5 (2026-06-01) — resolve the per-zone capsule size + shape once so the
        // chrome / label / badge below can branch on them (Tauri ZenCapsule).
        let size = CapsuleSize::parse(zone.capsule_size.as_ref());
        let shape = CapsuleShape::parse(zone.capsule_shape.as_ref());
        let is_minimal = matches!(shape, CapsuleShape::Minimal);
        // V-8 — compose hover + press into the final scale multiplier and
        // expand the pill rect about its center. Persisted geometry tokens
        // are NEVER mutated (hard constraint) — `scale_rect_centered`
        // returns a fresh `Rect` for paint only.
        //
        // Fix 8 (G5, VERIFIED) — `pill_scale_for` is a no-op: `HOVER_SCALE_DELTA`
        // and `PRESS_SCALE_DELTA` are both 0.0 (V-12 disabled pill scale), so
        // this returns EXACTLY 1.0 for any hover/press and `scaled_rect` ==
        // `layout.rect`. Tauri's ZenCapsule has no scale transform, so this
        // matches; left in place per V-12 (no scale re-enable).
        let scale = animator::pill_scale_for(hover_t, press_t);
        let scaled_rect = animator::scale_rect_centered(layout.rect, scale);
        let scaled_radius = layout.radius;
        // V21-C1 (2026-06-21) — Tauri's collapsed `.bento-zone--zen` carries
        // `box-shadow: var(--shadow-zen)`. Restore that contract through the
        // same feathered, allocation-free ShadowStack painter used by expanded
        // panels, keyed to the active theme rather than the static dark token.
        if is_minimal {
            // G5 (2026-06-01) — `minimal` shape (Tauri BentoZone.css:92-99
            // `.bento-zone--shape-minimal`): TRANSPARENT background, NO
            // backdrop blur, NO shadow/glow, just a 1px DASHED border at
            // rgba(255,255,255,0.2). Skip the acrylic + surface fills + neon
            // glow entirely and stroke a dashed outline instead of the solid
            // `border-zen` hairline. Corner radius is the resolved 8px.
            let dashed_border = Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.2,
            };
            self.stroke_rounded_rect_dashed(
                scaled_rect,
                fade_color(dashed_border, opacity),
                scaled_radius,
                1.0,
            )?;
        } else {
            // M6c — the `cyberpunk` neon `filter: drop-shadow` bloom on the
            // collapsed pill (`.bento-zone`), painted UNDER the glass+surface
            // fill and alongside the restored theme shadow.
            // Paint the exact geometry returned by `pill_layout_for_zone`.
            // The former medium/large-only 4-DIP vertical inset made the
            // settled pill 8 DIP shorter than morph t=0 and visibly swapped
            // the shell at both ends of the transition.
            let chrome_rect = scaled_rect;
            let chrome_radius = ordinary_zone_pill_chrome_radius(chrome_rect, scaled_radius);
            if let bento_nano_style::tokens::EffectTauri::Neon(n) = effect {
                self.draw_neon_glow(
                    chrome_rect,
                    [
                        fade_shadow(n.collapsed[0], opacity),
                        fade_shadow(n.collapsed[1], opacity),
                    ],
                    chrome_radius,
                )?;
            }
            self.draw_shadow_stack(
                chrome_rect,
                fade_shadow_stack(ordinary_zone_pill_shadow_stack(size, shadow_zen), opacity),
                chrome_radius,
            )?;
            // Frosted-backdrop (2026-06-01, real acrylic) — the collapsed pill
            // surface is now [blurred-desktop backdrop clipped to the capsule] +
            // [ONE `surface_zen` tint]. The old double layer (`ACRYLIC_FALLBACK`
            // + `surface_color`) over the SHARP wallpaper read as murk; Tauri
            // paints this same `surface_zen` 55% alpha OVER `blur(20px)`.
            // `fill_frosted_rect` degrades to the single tint when no backdrop.
            // V21-C9 — Tauri's ordinary `.bento-zone--zen` has no hover
            // background rule. Keep the base tint exactly `surface_zen`; hover
            // feedback belongs to stack-specific shadow/transform paths, not a
            // hidden RGB brighten on every ordinary capsule.
            let surface_color = collapsed_zen_surface_color(pal, hover_t);
            self.fill_frosted_rect_with_group_opacity(
                chrome_rect,
                surface_color,
                chrome_radius,
                opacity,
            )?;
            // M2 S2a (2026-05-29) — Tauri's `.zen-capsule` carries a 1px solid
            // `var(--border-zen)` = `rgba(255,255,255,0.1)` outline. nano drew
            // no stroke at all; added here so the capsule reads as glass with a
            // hairline edge. Pure-paint via the existing `stroke_rounded_rect`.
            self.stroke_rounded_rect(
                chrome_rect,
                fade_color(pal.border_zen, opacity),
                chrome_radius,
                1.0,
            )?;
        }
        // M2 S2b (2026-05-29) — the under-icon accent stripe was REMOVED.
        // Tauri's collapsed ZenCapsule has no such stripe (the 2px accent
        // border-top belongs to the EXPANDED body only). The zone accent is
        // still consulted below to tint the count badge (Tauri
        // `var(--zone-accent, --badge-bg)`).
        self.draw_zone_pill_content(zone, layout, display_count, opacity, pal)?;
        // V-9 round 3 (2026-05-21) — Wave H2 top-right status dot removed.
        //
        // G5 (2026-06-01), fix 7 — the V-14 HOVER-gated green dot that painted
        // over the badge on hover has ALSO been removed. Tauri's ZenCapsule has
        // NO hover badge change (ZenCapsule.css:10 only transitions
        // `background`); the count badge stays visible on hover. The v1.2.4
        // "reference frames 005-008" the old comment cited do not reproduce in
        // the live v1.3.0 source. No separate always-on status dot is painted
        // here (the geometry contract exposes only icon, title, and badge).
        let _ = (anim_now_ms, press_t, hover_t);
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
    /// channels / palette) are all genuinely distinct paint
    /// data. Geometry, tint, border, identity, actions and item cards consume
    /// the same monotonic `morph`; no second structural paint timeline exists.
    // #2 step 7 (2026-06-02) — `hover_t` (the V-8 PillHover channel sample, 0..1)
    // is threaded in so the +8% surface brighten the collapsed pill carries is
    // continuous across the pill→morph hand-off rather than snapping away. The
    // params are independent paint primitives; bundling adds indirection on a
    // hot per-zone call site, so allow the count.
    #[allow(clippy::too_many_arguments)]
    fn draw_zone_pill_morph(
        &mut self,
        app: &AppState,
        zone: &Zone,
        pill_layout: &ZonePillLayout,
        expanded_rect: bento_nano_style::Rect,
        morph: f32,
        hover_t: f32,
        pal: bento_nano_style::tokens::PaletteTauri,
        item_chrome: &item_card::ItemCardChrome,
        effect: bento_nano_style::tokens::EffectTauri,
    ) -> Result<(), RenderError> {
        // M6a — live theme palette passed in by `draw_zones` (§10).
        // Frosted-backdrop (2026-06-01) — `ACRYLIC_FALLBACK` dropped from the
        // import: the morph's old `ACRYLIC_FALLBACK` + flat `surface_zen` double
        // layer is replaced by one `fill_frosted_rect` (blur + a single tint
        // lerped zen→dialog), so the token is no longer referenced here.
        use crate::business::zen_capsule::CapsuleSize;
        // Geometry, identity, expanded content and hit bounds consume the same
        // monotonic morph. Keeping the clamp here makes endpoint math explicit
        // and protects against malformed persisted/transient state.
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
        let expanded_radius = app.active_theme_radius_tauri().expanded;
        let radius_px = zone_pill_geometry::morph_pill_radius(
            pill_layout.radius.top_left,
            expanded_radius,
            morph,
        );
        let border_radius = BorderRadius::all(radius_px);
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
        // Use the same shadow path as both settled endpoints. W13-B suppresses
        // fake blurred geometry there; the former direct fills bypassed that
        // fix and looked like a dark animation plate behind the real Zone.
        let shadows = app.active_theme_shadow_tauri();
        let collapsed_shadow = ordinary_zone_pill_shadow_stack(
            CapsuleSize::parse(zone.capsule_size.as_ref()),
            shadows.zen,
        );
        self.draw_shadow_stack(
            rect,
            lerp_shadow_stack(collapsed_shadow, shadows.expanded, morph_clamped),
            border_radius,
        )?;
        // Frosted-backdrop (2026-06-01) — real-acrylic morph surface: [blurred
        // desktop clipped to the morphing rect] + [ONE tint], replacing the old
        // `ACRYLIC_FALLBACK` + flat `surface_zen` double layer.
        //
        // Cross-fade the real settled endpoint colors along the same morph as
        // geometry. A separate 300ms tint channel was visible as a plate-layer
        // transition after the shell had already changed shape.
        // V21-C9 — the collapsed endpoint is the exact `surface_zen` token
        // even during hover. Tauri animates the background token itself; it
        // does not add an extra hover-brightened endpoint before the morph.
        let surface_zen = collapsed_zen_surface_color(pal, hover_t);
        let morph_tint = lerp_color(surface_zen, pal.surface_expanded, morph_clamped);
        self.fill_frosted_rect(rect, morph_tint, border_radius)?;
        self.stroke_rounded_rect(
            rect,
            lerp_color(pal.border_zen, pal.border_expanded, morph_clamped),
            border_radius,
            1.0,
        )?;
        if let Some(accent) = tauri_zone_accent_color(zone.accent_color.as_deref()) {
            self.draw_expanded_panel_accent_edge(
                rect,
                border_radius,
                with_alpha(accent, accent.a * morph_clamped),
            )?;
        }

        let morph_layout =
            expanded_zone_grid::expanded_zone_layout_for_rect(rect, zone.items.len());
        let live_zen_layout = zone_pill_geometry::pill_content_layout_in_rect(*pill_layout, rect);
        let identity_layout =
            morph_zen_content_to_header(live_zen_layout, &morph_layout, morph_clamped);
        // Tauri's outer `.bento-zone` owns `overflow: hidden`. Keep every child
        // on the same live morph surface too: during collapse the cards reflow
        // faster than their opacity reaches zero, and without this clip they can
        // briefly paint below the already-shrunken shell like a detached layer.
        self.push_clip(rect)?;
        let content_result = (|| -> Result<(), RenderError> {
            // Icon, title and count are one persistent identity row. Only
            // expanded-only actions/cards fade; the identity itself moves from the
            // capsule slots into the final header slots without a duplicate copy.
            self.draw_zone_pill_content(zone, &identity_layout, zone.items.len(), 1.0, pal)?;
            self.draw_expanded_panel_header(app, zone, &morph_layout, pal, morph_clamped, false)?;

            if morph_clamped > 0.0 {
                let item_label_group_px =
                    item_label_group_font_size(zone.items.iter().filter_map(|item| {
                        let card_rect =
                            highlight_overlay::item_card_rect_for_item_in_panel(zone, item, rect);
                        (card_rect.width > 0.0 && card_rect.height > 0.0).then_some((
                            item_label_visible_name(item.name.as_ref()),
                            (card_rect.width - 8.0).max(0.0),
                        ))
                    }));
                for item in &zone.items {
                    let card_rect =
                        highlight_overlay::item_card_rect_for_item_in_panel(zone, item, rect);
                    if card_rect.width <= 0.0 || card_rect.height <= 0.0 {
                        continue;
                    }
                    let item_fill = if item.file_missing {
                        item_chrome.missing_background
                    } else {
                        item_chrome.normal_background
                    };
                    // Tauri keeps BentoPanel mounted while the capsule is
                    // collapsed, so `.item-enter` runs only on the initial DOM
                    // mount. Replaying it on every hover expansion made the
                    // shell arrive first and the cards look like a second layer.
                    // The persistent Bento layer's shared alpha is the complete
                    // per-expand reveal contract.
                    self.draw_item_card(
                        item,
                        card_rect,
                        item_fill,
                        item_chrome,
                        0.0,
                        false,
                        1.0,
                        item_label_group_px,
                        morph_clamped,
                    )?;
                }
            }
            Ok(())
        })();
        let pop_result = self.pop_clip();
        content_result.and(pop_result)
    }

    // Geometric draw helper: the params are independent paint primitives
    // (rect, fill, chrome bundle, M3-A2 scale + M3-A3 hover ramp/press flag).
    // Bundling them into a struct adds indirection at the hot per-item call
    // sites for no real benefit — the conventional render-code shape, so allow it.
    #[allow(clippy::too_many_arguments)]
    fn draw_item_card(
        &mut self,
        item: &ZoneItem,
        base_rect: bento_nano_style::Rect,
        fill: Color,
        chrome: &item_card::ItemCardChrome,
        hover_t: f32,
        press_held: bool,
        scale: f32,
        label_font_px: f32,
        alpha: f32,
    ) -> Result<(), RenderError> {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return Ok(());
        }
        let radius = chrome.card_radius;
        let fade = |color: Color| with_alpha(color, color.a * alpha);
        let text = fade(chrome.text);
        let icon_text = fade(chrome.icon_text);
        let fill = fade(fill);
        let hover_background = fade(chrome.hover_background);
        let hover_border = fade(chrome.hover_border);
        let hover_shadow_inner = fade(chrome.hover_shadow_inner);
        let hover_shadow_outer = fade(chrome.hover_shadow_outer);
        // M3-A2 (2026-05-29) — apply the `item_card::card_scale_for` hover/press
        // multiplier as a Tauri-style centred `transform: scale()`. The card
        // surface AND its inner icon/label inset offsets all inflate/deflate
        // about the card's CENTRE so the glyph + label stay centred (a CSS
        // transform scales the whole subtree, not just the box). `scale == 1.0`
        // (idle / drag-ghost) collapses to the original geometry exactly.
        let mut card_rect = animator::scale_rect_centered(base_rect, scale);
        // FIX 1 (M3-A3) — Tauri `.item-card:hover { transform: translateY(-1px)
        // scale(1.02) }`: the lift rides the same 150ms ease-out ramp as the
        // scale. We offset the scaled rect's `y` by `CARD_HOVER_LIFT_DY *
        // hover_t` (0 at idle → -1px at full hover). Per CSS specificity the
        // `:active` rule respecifies `transform: scale(0.97)` (scale-only), so
        // the inherited lift is DROPPED while the pointer is actively held —
        // `press_held` mirrors that exactly. On release the lift returns.
        if !press_held {
            card_rect.y += item_card::CARD_HOVER_LIFT_DY * hover_t.clamp(0.0, 1.0);
        }
        // FIX 2 (M3-A3) — `:hover { box-shadow: var(--shadow-item-hover) }`: a
        // two-layer drop shadow (0 2 8 / 0 8 24 black) faded in by hover_t.
        // Painted BEHIND the card via the grow-and-fill idiom (one fill per
        // layer, back-to-front: the wider ambient layer first, the tighter
        // contact layer on top), §10 allocation-free — no per-frame heap, no
        // D2D blur effect. Skipped entirely at hover_t ≈ 0 (fill alpha guard).
        let hover_clamped = hover_t.clamp(0.0, 1.0);
        if hover_clamped > 0.0 {
            // Ambient: offset_y 8, blur 24.
            let ambient = bento_nano_style::Rect {
                x: card_rect.x - 24.0,
                y: card_rect.y + 8.0 - 24.0,
                width: card_rect.width + 48.0,
                height: card_rect.height + 48.0,
            };
            self.fill_rounded_rect(
                ambient,
                with_alpha(
                    chrome.hover_shadow_inner,
                    hover_shadow_inner.a * hover_clamped,
                ),
                radius,
            )?;
            // Contact: offset_y 2, blur 8.
            let contact = bento_nano_style::Rect {
                x: card_rect.x - 8.0,
                y: card_rect.y + 2.0 - 8.0,
                width: card_rect.width + 16.0,
                height: card_rect.height + 16.0,
            };
            self.fill_rounded_rect(
                contact,
                with_alpha(
                    chrome.hover_shadow_outer,
                    hover_shadow_outer.a * hover_clamped,
                ),
                radius,
            )?;
        }
        // FIX 2 (M3-A3) — `:hover { background: var(--surface-hover) }`: lerp the
        // base fill toward the hover surface by hover_t (premultiplied-alpha
        // lerp, §10 stack-only). At hover_t 0 this is `fill` exactly (idle /
        // missing / drag bg preserved); at 1.0 it is `--surface-hover`.
        let card_fill = fill.lerp(hover_background, hover_clamped);
        self.fill_rounded_rect(card_rect, card_fill, radius)?;
        // FIX 2 (M3-A3) — `:hover { border-color: var(--border-hover) }`: a 1px
        // stroke whose alpha lerps transparent → `--border-hover` by hover_t.
        // The normal card strokes no border, so this only appears on hover.
        if hover_clamped > 0.0 {
            let border = with_alpha(hover_border, hover_border.a * hover_clamped);
            self.stroke_rounded_rect(card_rect, border, radius, 1.0)?;
        }
        // FIX 3 (M3-A3, DEFERRED) — Tauri `:focus-visible { outline: 2px solid
        // var(--accent-blue); outline-offset: 2px; border-color: transparent }`.
        // nano tracks NO per-item KEYBOARD focus signal distinct from selection
        // (`ZoneItem` has no `selected`/`focused` field; `AppState` only tracks
        // `settings_focused_field` for the Settings text inputs). Building
        // focus-tracking plumbing is out of scope for this parity pass — paint
        // the ring once an item keyboard-focus channel lands.
        // V21-C3 — mirror Tauri's ItemIcon slot: a 36px/28px centred container
        // with the actual bitmap/glyph rendered at 24px/20px inside it.
        let (_icon_container_rect, icon_rect) =
            item_icon_slots_for_card(card_rect, item.is_wide, scale);
        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect, alpha)? {
            // Wave I2 / R4 — cache misses still use selected-stack line-art
            // icon families, never the old extension-keyed emoji text fallback.
            let kind =
                item_icon::fallback_icon_kind_for_item(item.icon_hash.as_ref(), item.path.as_ref());
            self.draw_icon_glyph(kind.as_str(), icon_rect, icon_text)?;
        }
        // V21-C3/V21-N108/V21-N110 — the label sits on the lower card text
        // rail and follows Tauri's full-text shrink contract (`useTextAbbr`),
        // not DWrite ellipsis trimming.
        let label_text = item_label_visible_name(item.name.as_ref());
        let label_rect = item_label_rect_for_card(card_rect, scale, label_font_px);
        // V21-C3/V21-N129 — weight stays Tauri's 400 contract; size and colour
        // follow the captured 2026-06-02 frame where source tokens conflict.
        // #1 step 13 / V21-N108 — the run is horizontally CENTERED, while the
        // layout box is pinned to the lower rail so DWrite top-near glyph ink
        // matches the WebView reference instead of drifting upward.
        self.draw_item_label_no_wrap(label_text, label_rect, text, label_font_px)?;
        Ok(())
    }

    /// Draw an item icon bitmap if the backend cache has bytes for the item's
    /// icon hash. Returns `false` when fallback text should be used.
    fn draw_item_bitmap(
        &mut self,
        icon_hash: &str,
        rect: bento_nano_style::Rect,
        opacity: f32,
    ) -> Result<bool, RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(true);
        }
        if icon_hash.is_empty()
            || icon_hash.starts_with("builtin:")
            || self.icon_bitmap_failures.contains(icon_hash)
        {
            return Ok(false);
        }

        if !self.icon_bitmaps.contains_key(icon_hash) {
            let Some(cache) = bento_nano_backend::icon::cache_handle() else {
                return Ok(false);
            };
            let Some(bytes) = cache.get(icon_hash) else {
                // Startup icon repair populates the cache off the UI thread.
                // A miss is therefore pending, not a permanent decode failure.
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
        d2d::draw_bitmap(&surface.ctx, &bitmap, d2d_rect, opacity)?;
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
            SETTINGS_BACKUP_ROW_VISIBLE_MAX, SETTINGS_PANEL_RADIUS, SETTINGS_PERF_ROW_COUNT,
            SETTINGS_RADIO_INNER_D, SETTINGS_RADIO_OUTER_D, SETTINGS_ROW_PAD_X,
            SETTINGS_SCROLLBAR_W, SETTINGS_SLIDER_THUMB_D, SETTINGS_SOURCE_ROW_VISIBLE_MAX,
            SETTINGS_TOP_TOGGLE_COUNT, SETTINGS_ZONE_DISPLAY_MODE_COUNT, SettingsBodyFlags,
            UpdaterHeightKind, settings_backup_actions_row_rect,
            settings_backup_create_button_rect, settings_backup_description_rect,
            settings_backup_entry_row_rect, settings_backup_label_rect,
            settings_backup_restore_button_rect, settings_backup_status_rect,
            settings_body_content_height, settings_body_rect, settings_cancel_button_rect,
            settings_close_button_rect_m1, settings_crash_max_retries_row_rect,
            settings_crash_restart_row_rect, settings_crash_window_row_rect,
            settings_desktop_path_input_rect, settings_desktop_path_label_rect,
            settings_display_mode_copy_label_rect, settings_display_mode_hint_rect,
            settings_footer_rect, settings_general_label_rect, settings_header_rect,
            settings_hibernate_slider_rect, settings_hibernate_slider_row_rect,
            settings_language_chevron_rect, settings_language_chip_label_rect,
            settings_language_chip_rect, settings_language_row_rect, settings_panel_fills_host,
            settings_panel_rect_m1, settings_paths_label_rect, settings_performance_label_rect,
            settings_performance_slider_rect, settings_performance_slider_row_rect,
            settings_safe_start_row_rect, settings_save_button_rect, settings_scrollbar_thumb_rect,
            settings_source_row_rect, settings_sources_label_rect,
            settings_sources_refresh_button_rect, settings_sources_reserve_delta,
            settings_startup_high_priority_row_rect, settings_startup_label_rect,
            settings_startup_toggle_hit_rect, settings_stealth_buttons_row_rect,
            settings_stealth_error_block_rect, settings_stealth_label_rect,
            settings_stealth_mirror_row_rect, settings_stealth_onedrive_block_rect,
            settings_stealth_pill_rect, settings_stealth_reapply_button_rect,
            settings_stealth_refresh_button_rect, settings_stealth_retry_row_rect,
            settings_stealth_schema_row_rect, settings_stealth_status_row_rect,
            settings_stepper_input_rect, settings_stepper_value_rect, settings_top_toggle_hit_rect,
            settings_top_toggle_row_rect, settings_updater_auto_download_hit_rect,
            settings_updater_auto_download_row_rect, settings_updater_button_rect,
            settings_updater_buttons_row_rect, settings_updater_frequency_chip_rect,
            settings_updater_frequency_row_rect, settings_updater_label_rect,
            settings_updater_middle_block_rect, settings_updater_pill_rect,
            settings_updater_progress_track_rect, settings_updater_status_row_rect,
            settings_watch_label_rect, settings_watch_textarea_rect,
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
        // P1 (#7 fix wave 2026-06-01) — wall-clock sampled ONCE per Settings
        // paint (same `GetTickCount` pattern `draw_zones` uses for the pill
        // animator, allocation-free §10). Threaded into the §2/§10 text-field
        // caret blink so a focused caret toggles at the Windows ~530ms cadence.
        // The frame-pump keeps redrawing while a field is focused (the shell
        // ORs `settings_focused_field != None` into `any_active`), so this value
        // advances frame to frame.
        // SAFETY: `GetTickCount` is total + thread-safe.
        let settings_now_ms =
            unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        // P1 — caret is ON for the first half of each ~1060ms blink period.
        let caret_on = settings_caret_on(settings_now_ms);
        // Tauri's modal uses the active theme's expanded glass token. Auxiliary
        // HWNDs deliberately do not sample Main's monitor-sized backdrop: a
        // movable panel would otherwise drag a detached wallpaper snapshot.
        // Therefore the native fallback must be fully opaque. Even a 0.96
        // surface leaves high-contrast foreground-window edges visible as a
        // sharp vertical seam through the Settings card when Acrylic is
        // unavailable.
        let panel_bg = palette.surface_expanded;
        let has_panel_backdrop = self.backdrop_brush.is_some();
        let panel_fallback_bg = opaque_auxiliary_surface(panel_bg);
        let title_color = palette.text_primary;
        let label_color = palette.text_secondary;
        let accent_on = palette.accent_blue;
        // Native controls share one polarity-aware semantic derivation.  A
        // literal white overlay works on dark themes but disappears on Order,
        // Editorial, Neo and Frosted surfaces.
        let controls = palette.control_palette();
        let track_off = controls.track_off;
        let chip_bg = controls.fill;
        let chip_border = controls.border;
        let toggle_knob_color = controls.knob;
        let divider_color = controls.divider;
        let panel_radius = bento_nano_style::BorderRadius::all(SETTINGS_PANEL_RADIUS);
        // M6b — per-theme card radius for the Settings chip surfaces.
        let chip_radius_tokens = app.active_theme_radius_tauri();
        let chip_radius = bento_nano_style::BorderRadius::all(chip_radius_tokens.card);
        let btn_radius = bento_nano_style::BorderRadius::all(8.0);

        // RC-4 Gap 2 — derive a layout viewport from backbuffer + base_scale.
        let base_scale = self.base_scale.max(0.01);
        let viewport = bento_nano_style::Size {
            width: (self.width as f32 / base_scale).max(1.0),
            height: (self.height as f32 / base_scale).max(1.0),
        };

        // A synthetic wide overlay viewport still receives the reference scrim.
        // The production Settings HWND is the card itself; painting a rectangular
        // scrim there filled the transparent rounded corners with a black slab.
        if !settings_panel_fills_host(viewport) {
            let scrim_rect = bento_nano_style::Rect {
                x: 0.0,
                y: 0.0,
                width: viewport.width,
                height: viewport.height,
            };
            self.fill_rounded_rect(
                scrim_rect,
                with_alpha(bento_nano_style::Color::BLACK, 0.50),
                bento_nano_style::BorderRadius::ZERO,
            )?;
        }

        let panel = settings_panel_rect_m1(viewport);

        let open_progress = app.settings_open_animation_progress_at(settings_now_ms);
        let open_eased = crate::state::settings_open_animation_ease(open_progress);
        let open_scale = crate::state::settings_open_animation_scale(open_eased);
        let open_transform_active = (open_scale - 1.0).abs() > f32::EPSILON;
        if open_transform_active {
            let open_transform = scale_about_rect_center_matrix(base_scale, panel, open_scale);
            self.set_logical_transform_override(Some(open_transform))?;
        }

        let settings_paint = (|| -> Result<(), RenderError> {
            // 2) Panel card — blur the desktop snapshot, reapply the overlay's
            // 50% dimming inside the clipped card, then add the theme glass.
            // This mirrors CSS backdrop-filter ordering and avoids both sharp
            // text bleed and the old opaque black slab.
            if has_panel_backdrop {
                self.fill_frosted_rect(
                    panel,
                    with_alpha(bento_nano_style::Color::BLACK, 0.50),
                    panel_radius,
                )?;
                self.fill_rounded_rect(panel, panel_bg, panel_radius)?;
            } else {
                self.fill_rounded_rect(panel, panel_fallback_bg, panel_radius)?;
            }
            let panel_border = bento_nano_style::Rect {
                x: panel.x + 0.5,
                y: panel.y + 0.5,
                width: (panel.width - 1.0).max(0.0),
                height: (panel.height - 1.0).max(0.0),
            };
            self.stroke_rounded_rect(
                panel_border,
                palette.border_expanded,
                bento_nano_style::BorderRadius::all((SETTINGS_PANEL_RADIUS - 0.5).max(0.0)),
                1.0,
            )?;

            // 3) Header (sticky, 52 DIP) — title + close ×.
            let header = settings_header_rect(viewport);
            let title_rect = bento_nano_style::Rect {
                x: header.x + SETTINGS_ROW_PAD_X,
                y: header.y + (header.height - 20.0) * 0.5,
                width: header.width * 0.5,
                height: 20.0,
            };
            self.draw_text_no_wrap_with_style(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_TITLE),
                title_rect,
                title_color,
                16.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let close_rect = settings_close_button_rect_m1(viewport);
            let close_chrome = panel_header_button_chrome(
                palette,
                PanelHeaderButtonKind::Close,
                app.settings_close_hover.get(),
            );
            if let Some(background) = close_chrome.background {
                self.fill_rounded_rect(close_rect, background, BorderRadius::all(8.0))?;
            }
            self.draw_icon_glyph(
                IconKind::X.as_str(),
                centered_square_rect(close_rect, 16.0),
                close_chrome.glyph,
            )?;
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

                let general_label = settings_general_label_rect(viewport, scroll);
                if row_visible(general_label, body) {
                    self.draw_settings_group_title(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_GENERAL,
                        ),
                        general_label,
                        palette.text_muted,
                    )?;
                }

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
                    self.draw_settings_text(
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
                        toggle_knob_color,
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
                    self.draw_settings_text(
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
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_EN_US,
                            )
                        } else {
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::LOCALE_LABEL_ZH_CN,
                            )
                        };
                    self.draw_settings_text_no_wrap(
                        locale_label,
                        settings_language_chip_label_rect(viewport, scroll),
                        title_color,
                    )?;
                    self.draw_settings_text_no_wrap(
                        "▾",
                        settings_language_chevron_rect(viewport, scroll),
                        label_color,
                    )?;
                }

                // §4 DisplayMode group (G3 parity 2026-06-01) — promoted out of the
                // General band into its own `settings-group` between §3 Appearance and
                // §5 Performance. Because §4 roots at the FIXED source-reserve baseline
                // (it anchors off §3 Appearance, like Performance §5), it must paint with
                // the reserve-FOLDED `scroll` — so the paint block lives AFTER the fold,
                // adjacent to the §3 Appearance block near the end of this closure (paint
                // ==hit SSoT; see the `§4 DisplayMode` block below the Appearance grid).

                // ── Round-2 M2 sections ──────────────────────────────────────────

                let paths_label = settings_paths_label_rect(viewport, scroll);
                if row_visible(paths_label, body) {
                    self.draw_settings_group_title(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_PATHS,
                        ),
                        paths_label,
                        palette.text_muted,
                    )?;
                }

                // 桌面源 label (M1i fidelity — Tauri `.settings-row__label` ABOVE the
                // `.desktop-source-list`; refresh button is now the list's LAST child,
                // painted after the cards below, `SettingsPanel.tsx:317-361`).
                let source_count = app.desktop_sources.borrow().len();
                let sources_label = settings_sources_label_rect(viewport, scroll);
                if row_visible(sources_label, body) {
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_SOURCES,
                        ),
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
                let card_bg = palette.neutral_overlay(0.04);
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
                    self.draw_text_no_wrap_with_style(
                        icon_glyph,
                        icon_rect,
                        bento_nano_style::Color::WHITE,
                        12.0,
                        600,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Center,
                            v: dwrite::VAlign::Center,
                        },
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
                        self.draw_text_no_wrap_with_style(
                            badge_upper.as_str(),
                            badge_rect,
                            palette.accent_green,
                            BADGE_FONT,
                            600,
                            1.0,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Center,
                                v: dwrite::VAlign::Center,
                            },
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
                let refresh_btn =
                    settings_sources_refresh_button_rect(viewport, scroll, source_count);
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
                    self.draw_text_no_wrap_with_style(
                        "\u{21BB}",
                        refresh_btn,
                        title_color,
                        14.0,
                        400,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Center,
                            v: dwrite::VAlign::Center,
                        },
                    )?;
                }

                // 桌面路径 label + input (reflows below the live source stack).
                let path_label = settings_desktop_path_label_rect(viewport, scroll, source_count);
                if row_visible(path_label, body) {
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SECTION_DESKTOP_PATH,
                        ),
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
                    self.draw_settings_text_no_wrap(path_text.as_str(), text_rect, title_color)?;
                    drop(path_text);
                }

                // 监控值 label + textarea (reflows below the live source stack).
                let watch_label = settings_watch_label_rect(viewport, scroll, source_count);
                if row_visible(watch_label, body) {
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SECTION_WATCH_VALUES,
                        ),
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
                        self.draw_settings_text(
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
                        self.draw_settings_text(watch_text.as_str(), text_rect, title_color)?;
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
                let slider_track_radius = bento_nano_style::BorderRadius::all(2.0);
                let slider_thumb_radius =
                    bento_nano_style::BorderRadius::all(SETTINGS_SLIDER_THUMB_D * 0.5);

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
                    self.draw_settings_group_title(
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
                    self.draw_settings_text(
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
                    self.draw_text_no_wrap_with_style(
                        value_text.as_str(),
                        value_rect,
                        title_color,
                        crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
                        crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
                        crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Trailing,
                            v: dwrite::VAlign::Near,
                        },
                    )?;
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
                    // Tauri `.settings-slider::-webkit-slider-thumb` uses the
                    // active accent, while only toggle-switch thumbs are white.
                    self.fill_rounded_rect(thumb, accent_on, slider_thumb_radius)?;
                }

                // Startup management group title.
                let startup_label = settings_startup_label_rect(viewport, scroll);
                if row_visible(startup_label, body) {
                    self.draw_settings_group_title(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_STARTUP,
                        ),
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
                    self.draw_settings_text(
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
                        toggle_knob_color,
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
                    self.draw_settings_text(
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART,
                        ),
                        label_rect,
                        label_color,
                    )?;
                    let switch = toggle_switch_in_rect(settings_startup_toggle_hit_rect(crash_row));
                    self.fill_rounded_rect(
                        switch.track,
                        if crash_restart_on {
                            accent_on
                        } else {
                            track_off
                        },
                        BorderRadius::all(switch.track_radius()),
                    )?;
                    self.fill_rounded_rect(
                        switch.knob(crash_restart_on),
                        toggle_knob_color,
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTING_CRASH_RESTART_DESC,
                        ),
                        crash_desc,
                        with_alpha(label_color, 0.7),
                    )?;
                }

                // Rows 2/3 — crash number inputs, ONLY when crash_restart_on.
                // The 72×30 shell matches Tauri `.settings-row__number-input`;
                // the existing side targets retain decrement/increment behaviour.
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
                        self.draw_settings_text(
                            bento_nano_style::t(bento_nano_style::StringId(label_id)),
                            label_rect,
                            label_color,
                        )?;
                        let val_rect = settings_stepper_value_rect(row);
                        let input_rect = settings_stepper_input_rect(row);
                        self.fill_rounded_rect(input_rect, chip_bg, btn_radius)?;
                        self.stroke_rounded_rect(input_rect, chip_border, btn_radius, 1.0)?;
                        let buf = smol_str::SmolStr::new(value.to_string());
                        self.draw_text_no_wrap_with_style(
                            buf.as_str(),
                            val_rect,
                            title_color,
                            crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
                            crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
                            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Center,
                                v: dwrite::VAlign::Center,
                            },
                        )?;
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
                    self.draw_settings_text(
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
                        toggle_knob_color,
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTING_SAFE_START_HIBERNATION_DESC,
                        ),
                        safe_desc,
                        with_alpha(label_color, 0.7),
                    )?;
                }

                // Row 5 — 恢复延迟 SliderRow, ONLY when safe_start_on.
                if safe_start_on {
                    let row =
                        settings_hibernate_slider_row_rect(viewport, scroll, crash_restart_on);
                    if row_visible(row, body) {
                        let value = app.hibernate_resume_delay_ms.get().clamp(
                            crate::state::HIBERNATE_DELAY_MIN_MS,
                            crate::state::HIBERNATE_DELAY_MAX_MS,
                        );
                        let label_rect = bento_nano_style::Rect {
                            x: row.x,
                            y: row.y + 4.0,
                            width: row.width * 0.6,
                            height: 16.0,
                        };
                        self.draw_settings_text(
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
                        self.draw_text_no_wrap_with_style(
                            value_text.as_str(),
                            value_rect,
                            title_color,
                            crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
                            crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
                            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Trailing,
                                v: dwrite::VAlign::Near,
                            },
                        )?;
                        let track =
                            settings_hibernate_slider_rect(viewport, scroll, crash_restart_on);
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
                        let frac = ((value - crate::state::HIBERNATE_DELAY_MIN_MS) as f32 / span)
                            .clamp(0.0, 1.0);
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
                        self.fill_rounded_rect(thumb, accent_on, slider_thumb_radius)?;
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
                let stealth_label =
                    settings_stealth_label_rect(viewport, scroll, crash_restart_on, safe_start_on);
                if row_visible(stealth_label, body) {
                    self.draw_settings_text(
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_LABEL,
                        ),
                        label_rect,
                        label_color,
                    )?;
                    let pill = settings_stealth_pill_rect(status_row);
                    let pill_radius = bento_nano_style::BorderRadius::all(pill.height * 0.5);
                    // Keep status colour in the text and use a restrained tint for the
                    // capsule surface. A near-solid fill made the small status pill read
                    // like a primary action and amplified any text-alignment error.
                    let (pill_bg, pill_fg, pill_label_id) = match stealth_snapshot.as_ref() {
                        Some(s) => {
                            let level = StatusLevel::from_status(s);
                            let fg = match level {
                                StatusLevel::Applied => palette.accent_green,
                                StatusLevel::Pending => palette.accent_orange,
                                StatusLevel::Failed => palette.accent_red,
                            };
                            (with_alpha(fg, 0.18), fg, level.label_id())
                        }
                        None => (
                            controls.disabled_fill,
                            palette.text_muted,
                            bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_PENDING,
                        ),
                    };
                    self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(pill_label_id),
                        pill,
                        pill_fg,
                        10.0,
                        600,
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::STEALTH_SCHEMA_VERSION,
                        ),
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
                    self.draw_settings_row_value(
                        schema_text.as_str(),
                        value_rect,
                        palette.text_muted,
                    )?;
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
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::STEALTH_MIRROR_HEALTHY,
                        ),
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
                    self.draw_settings_row_value(
                        bento_nano_style::t(mirror_id),
                        value_rect,
                        palette.text_muted,
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
                        self.draw_settings_text(
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::STEALTH_RETRY_COUNT,
                            ),
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
                        self.draw_settings_row_value(
                            retry_text.as_str(),
                            value_rect,
                            palette.text_muted,
                        )?;
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
                        self.draw_settings_text(
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::STEALTH_LAST_ERROR,
                            ),
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
                                self.draw_settings_text(
                                    err,
                                    err_rect,
                                    with_alpha(palette.accent_red, 0.9),
                                )?;
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
                    self.stroke_rounded_rect(refresh_btn, chip_border, btn_radius, 1.0)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_REFRESH_BTN),
                        refresh_btn,
                        title_color,
                        12.0,
                        500,
                    )?;
                    let reapply_btn = settings_stealth_reapply_button_rect(stealth_btn_row);
                    self.fill_rounded_rect(reapply_btn, accent_on, btn_radius)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::STEALTH_REAPPLY_BTN),
                        reapply_btn,
                        controls.on_accent,
                        12.0,
                        500,
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
                        let od_bg = with_alpha(palette.accent_orange, 0.12);
                        self.fill_rounded_rect(od_block, od_bg, chip_radius)?;
                        let text_rect = bento_nano_style::Rect {
                            x: od_block.x + 10.0,
                            y: od_block.y + 8.0,
                            width: (od_block.width - 20.0).max(0.0),
                            height: (od_block.height - 16.0).max(0.0),
                        };
                        self.draw_settings_text(
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
                    self.draw_settings_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CARD_TITLE),
                        updater_label,
                        label_color,
                    )?;
                }
                // Row 0 — status (label + colored pill), always shown.
                let upd_value_x_frac = 0.5_f32;
                let upd_status_row =
                    settings_updater_status_row_rect(viewport, scroll, &updater_flags);
                if row_visible(upd_status_row, body) {
                    let label_rect = bento_nano_style::Rect {
                        x: upd_status_row.x,
                        y: upd_status_row.y + (upd_status_row.height - 16.0) * 0.5,
                        width: upd_status_row.width * upd_value_x_frac,
                        height: 16.0,
                    };
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::UPDATER_STATUS_LABEL,
                        ),
                        label_rect,
                        label_color,
                    )?;
                    // Status text owns the semantic colour; the pill keeps only a soft
                    // tint so it remains a status indicator rather than a primary CTA.
                    let pill = settings_updater_pill_rect(upd_status_row);
                    let pill_radius = bento_nano_style::BorderRadius::all(pill.height * 0.5);
                    let (pill_bg, pill_fg) =
                        match upd::UpdaterPillKind::from_status(&updater_status) {
                            upd::UpdaterPillKind::UpToDate | upd::UpdaterPillKind::Ready => {
                                let fg = palette.accent_green;
                                (with_alpha(fg, 0.16), fg)
                            }
                            upd::UpdaterPillKind::Busy | upd::UpdaterPillKind::Active => {
                                let fg = palette.accent_blue;
                                (with_alpha(fg, 0.16), fg)
                            }
                            upd::UpdaterPillKind::Skipped => {
                                (controls.disabled_fill, palette.text_muted)
                            }
                            upd::UpdaterPillKind::Error => {
                                let fg = palette.accent_red;
                                (with_alpha(fg, 0.16), fg)
                            }
                        };
                    self.fill_rounded_rect(pill, pill_bg, pill_radius)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(upd::updater_status_label_id(&updater_status)),
                        pill,
                        pill_fg,
                        11.0,
                        600,
                    )?;
                }
                // Middle block — version line (Available/Ready/Installing/Skipped),
                // progress bar (Downloading), or error line (Error). Mutually
                // exclusive; StatusOnly paints nothing (zero-height block).
                let upd_middle =
                    settings_updater_middle_block_rect(viewport, scroll, &updater_flags);
                if upd_middle.height > 0.0 && row_visible(upd_middle, body) {
                    match updater_flags.updater_kind {
                        UpdaterHeightKind::Versioned => {
                            let label_rect = bento_nano_style::Rect {
                                x: upd_middle.x,
                                y: upd_middle.y + (upd_middle.height - 16.0) * 0.5,
                                width: upd_middle.width * upd_value_x_frac,
                                height: 16.0,
                            };
                            self.draw_settings_text(
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
                                self.draw_settings_row_value(
                                    version.as_str(),
                                    value_rect,
                                    palette.text_muted,
                                )?;
                            }
                        }
                        UpdaterHeightKind::Downloading => {
                            // Track + filled portion. When the total is unknown the
                            // fraction is None → paint a muted full-width track only
                            // (indeterminate cue), never a panic / divide-by-zero.
                            let track = settings_updater_progress_track_rect(
                                viewport,
                                scroll,
                                &updater_flags,
                            );
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
                                self.draw_settings_text(
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
                let upd_btn_row =
                    settings_updater_buttons_row_rect(viewport, scroll, &updater_flags);
                if row_visible(upd_btn_row, body) {
                    // Col 0 — 检查更新 (always).
                    let check_btn = settings_updater_button_rect(upd_btn_row, 0);
                    let updater_action_bg = with_alpha(accent_on, 0.18);
                    let updater_action_border = with_alpha(accent_on, 0.38);
                    self.fill_rounded_rect(check_btn, updater_action_bg, btn_radius)?;
                    self.stroke_rounded_rect(check_btn, updater_action_border, btn_radius, 1.0)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::UPDATER_CHECK_NOW),
                        check_btn,
                        accent_on,
                        12.0,
                        500,
                    )?;
                    // Col 1 — 下载 (Available) or 安装并重启 (Ready), accent-filled.
                    if upd::updater_show_download(&updater_status) {
                        let dl_btn = settings_updater_button_rect(upd_btn_row, 1);
                        self.fill_rounded_rect(dl_btn, updater_action_bg, btn_radius)?;
                        self.stroke_rounded_rect(dl_btn, updater_action_border, btn_radius, 1.0)?;
                        self.draw_settings_button_text(
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::UPDATER_DOWNLOAD,
                            ),
                            dl_btn,
                            accent_on,
                            12.0,
                            500,
                        )?;
                    } else if upd::updater_show_install(&updater_status) {
                        let install_btn = settings_updater_button_rect(upd_btn_row, 1);
                        self.fill_rounded_rect(install_btn, updater_action_bg, btn_radius)?;
                        self.stroke_rounded_rect(
                            install_btn,
                            updater_action_border,
                            btn_radius,
                            1.0,
                        )?;
                        self.draw_settings_button_text(
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::UPDATER_INSTALL_RESTART,
                            ),
                            install_btn,
                            accent_on,
                            12.0,
                            500,
                        )?;
                    }
                    // Col 2 — 跳过此版本 (Available/Ready), neutral chip.
                    if upd::updater_show_skip(&updater_status) {
                        let skip_btn = settings_updater_button_rect(upd_btn_row, 2);
                        self.fill_rounded_rect(skip_btn, chip_bg, btn_radius)?;
                        self.stroke_rounded_rect(skip_btn, chip_border, btn_radius, 1.0)?;
                        self.draw_settings_button_text(
                            bento_nano_style::t(
                                bento_nano_style::i18n_zh_cn::ids::UPDATER_SKIP_VERSION,
                            ),
                            skip_btn,
                            title_color,
                            12.0,
                            500,
                        )?;
                    }
                }
                // Prefs row — 检查频率 cycling chip (Daily/Weekly/Manual).
                let upd_freq_row =
                    settings_updater_frequency_row_rect(viewport, scroll, &updater_flags);
                if row_visible(upd_freq_row, body) {
                    let label_rect = bento_nano_style::Rect {
                        x: upd_freq_row.x,
                        y: upd_freq_row.y + (upd_freq_row.height - 16.0) * 0.5,
                        width: upd_freq_row.width * upd_value_x_frac,
                        height: 16.0,
                    };
                    self.draw_settings_text(
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
                    self.draw_settings_button_text(
                        bento_nano_style::t(freq_id),
                        chip,
                        title_color,
                        12.0,
                        500,
                    )?;
                }
                // Prefs row — 后台静默下载 toggle.
                let upd_auto_row =
                    settings_updater_auto_download_row_rect(viewport, scroll, &updater_flags);
                if row_visible(upd_auto_row, body) {
                    let label_rect = bento_nano_style::Rect {
                        x: upd_auto_row.x,
                        y: upd_auto_row.y + (upd_auto_row.height - 16.0) * 0.5,
                        width: upd_auto_row.width * 0.7,
                        height: 16.0,
                    };
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::UPDATER_AUTO_DOWNLOAD,
                        ),
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
                        toggle_knob_color,
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
                let backup_flags = updater_flags
                    .with_backup_rows(backup_visible)
                    .with_backup_status(backup_status_snapshot.is_some())
                    .with_encryption_status(app.settings_encryption_status.borrow().is_some());
                let backup_label = settings_backup_label_rect(viewport, scroll, &backup_flags);
                if row_visible(backup_label, body) {
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_CARD_TITLE),
                        backup_label,
                        title_color,
                        15.0,
                        600,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Leading,
                            v: dwrite::VAlign::Center,
                        },
                    )?;
                }
                // Description line — always shown.
                let backup_desc = settings_backup_description_rect(viewport, scroll, &backup_flags);
                if row_visible(backup_desc, body) {
                    self.draw_text_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::BACKUP_CARD_DESCRIPTION,
                        ),
                        backup_desc,
                        label_color,
                        12.0,
                        400,
                        1.0,
                    )?;
                }
                // Tauri exposes one create action; list refresh remains automatic.
                let backup_actions =
                    settings_backup_actions_row_rect(viewport, scroll, &backup_flags);
                if row_visible(backup_actions, body) {
                    let create_btn = settings_backup_create_button_rect(backup_actions);
                    let create_radius = BorderRadius::all(6.0);
                    let create_accent = accent_on;
                    self.fill_rounded_rect(
                        create_btn,
                        with_alpha(create_accent, 0.18),
                        create_radius,
                    )?;
                    self.stroke_rounded_rect(create_btn, controls.border, create_radius, 1.0)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_CREATE_NOW),
                        create_btn,
                        create_accent,
                        12.0,
                        500,
                    )?;
                }
                // Info/error line — only when a status is set. Success → green, error
                // → red (mirrors the widget-tree card's status colours).
                if let Some(status) = backup_status_snapshot.as_ref() {
                    let backup_status_row =
                        settings_backup_status_rect(viewport, scroll, &backup_flags);
                    if row_visible(backup_status_row, body) {
                        let is_error =
                            matches!(status, crate::state::SettingsBackupStatus::Error(_));
                        let status_color = if is_error {
                            with_alpha(palette.accent_red, 0.9)
                        } else {
                            with_alpha(palette.accent_green, 0.9)
                        };
                        self.draw_settings_text(
                            bkp::backup_status_text(status),
                            backup_status_row,
                            status_color,
                        )?;
                    }
                }
                // Backup list — N entry rows (file·size + 恢复) or one backupEmpty
                // placeholder. Both branches anchor off the reserved status slot so the
                // list lines up whether or not a status line painted.
                if bkp::backup_list_is_empty(&backup_entries) {
                    let empty_row =
                        settings_backup_entry_row_rect(viewport, scroll, &backup_flags, 0);
                    if row_visible(empty_row, body) {
                        self.draw_text_no_wrap_with_style(
                            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_EMPTY),
                            empty_row,
                            palette.text_muted,
                            12.0,
                            400,
                            1.0,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Center,
                                v: dwrite::VAlign::Center,
                            },
                        )?;
                    }
                } else {
                    for (entry_index, entry) in backup_entries
                        .iter()
                        .take(SETTINGS_BACKUP_ROW_VISIBLE_MAX)
                        .enumerate()
                    {
                        let entry_row = settings_backup_entry_row_rect(
                            viewport,
                            scroll,
                            &backup_flags,
                            entry_index,
                        );
                        if !row_visible(entry_row, body) {
                            continue;
                        }
                        let entry_radius = BorderRadius::all(6.0);
                        self.fill_rounded_rect(
                            entry_row,
                            palette.neutral_overlay(0.04),
                            entry_radius,
                        )?;
                        let restore_btn = settings_backup_restore_button_rect(entry_row);
                        let info_width = (restore_btn.x - entry_row.x - 20.0).max(0.0);
                        let timestamp_rect = bento_nano_style::Rect {
                            x: entry_row.x + 12.0,
                            y: entry_row.y + 5.0,
                            width: info_width,
                            height: 16.0,
                        };
                        let timestamp = bkp::format_timestamp(entry.id.as_str());
                        self.draw_text_no_wrap_with_style(
                            timestamp.as_str(),
                            timestamp_rect,
                            title_color,
                            12.0,
                            400,
                            1.0,
                            dwrite::TextAlign::DEFAULT,
                        )?;
                        let size_rect = bento_nano_style::Rect {
                            x: timestamp_rect.x,
                            y: timestamp_rect.bottom(),
                            width: info_width,
                            height: 14.0,
                        };
                        let size = bkp::format_size(entry.size_bytes);
                        self.draw_text_no_wrap_with_style(
                            size.as_str(),
                            size_rect,
                            palette.text_muted,
                            11.0,
                            400,
                            1.0,
                            dwrite::TextAlign::DEFAULT,
                        )?;
                        let restore_radius = BorderRadius::all(6.0);
                        self.fill_rounded_rect(restore_btn, controls.fill, restore_radius)?;
                        self.stroke_rounded_rect(
                            restore_btn,
                            controls.border,
                            restore_radius,
                            1.0,
                        )?;
                        self.draw_settings_button_text(
                            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BACKUP_RESTORE),
                            restore_btn,
                            title_color,
                            12.0,
                            400,
                        )?;
                    }
                }

                // ── M7 — Encryption §10 card (`EncryptionCard.tsx`) ─────────────
                //
                // Slots BETWEEN Backup §9 and Plugins §11, matching the Tauri
                // `<BackupCard/><EncryptionCard/>` adjacency. Fixed-height card (no
                // variable rows) painted on top of the already-wired passphrase backend.
                // Controls (top→bottom): section label / OneDrive description / current-
                // mode row / 3-button mode grid (active button accent-highlighted) /
                // passphrase row (LEFT label cell + RIGHT masked input box — P4) / hint
                // line / status banner (error red / success green). The mode-button
                // geometry + the passphrase label/input rects come from the
                // `settings_encryption_*_rect` helpers (paint==hit SSoT).
                //
                // #7 fix wave 2026-06-01 — Tauri `EncryptionCard.tsx`/`.css` 1:1 parity:
                //   P1  caret BLINKS at ~530ms (`settings_now_ms` threaded in; the prior
                //       "no per-frame clock" claim was false — the shell pump keeps
                //       redrawing while a field is focused), still allocation-free (§10);
                //   P2  current-mode VALUE uses the SAME label source as the mode-button
                //       TITLES (`encryption_mode_button_title_id` → Passphrase = id 236);
                //   P3  literal ':' after the current-mode label;
                //   P4  passphrase LABEL painted left of the input;
                //   P5  inactive buttons ALWAYS stroke rgba(255,255,255,0.08) + fill
                //       rgba(255,255,255,0.04); active fill rgba(96,165,250,0.18) + #60a5fa;
                //   P6  unfocused input ALWAYS strokes rgba(255,255,255,0.12) + fills
                //       rgba(255,255,255,0.06);
                //   P7  active button TITLE stays text_primary (NOT recolored blue);
                //   P8  current-mode VALUE is bold (weight 700, `<strong>`);
                //   P11 description = text_secondary (not text_muted);
                //   P16 placeholder = text_primary @ 0.45 alpha.
                // The mask string is built once per paint into the reusable `mask_scratch`
                // buffer + the caret glyph is appended only when `caret_on` (no per-frame
                // heap alloc — §10). NEVER paints the literal passphrase.
                use crate::settings_panel::{
                    SETTINGS_ENCRYPTION_MODE_COUNT, settings_encryption_current_mode_rect,
                    settings_encryption_desc_rect, settings_encryption_hint_rect,
                    settings_encryption_label_rect, settings_encryption_mode_button_rect,
                    settings_encryption_passphrase_input_rect,
                    settings_encryption_passphrase_label_rect, settings_encryption_status_rect,
                };
                use crate::state::SettingsTextField;
                // Live encryption state, read once (Copy / cheap clones) so no RefCell
                // borrow spans the fallible paint calls below (mirrors the Backup/Stealth
                // snapshot pattern).
                let enc_mode = app.encryption_mode.get();
                let enc_status_snapshot = app.settings_encryption_status.borrow().clone();
                let enc_passphrase_focused = app.passphrase_entry_active.get()
                    && matches!(
                        app.settings_focused_field.get(),
                        SettingsTextField::Passphrase
                    );
                // Masked passphrase: number of dots = scalar count of the draft. Built
                // into a reusable scratch String (cleared, never freed) so the paint
                // path stays allocation-light (§10). NEVER the literal passphrase.
                let enc_pass_len = app.passphrase_draft.borrow().chars().count().min(128);
                // The Tauri card authored white overlays for its dark default.
                // Nano derives equivalent neutral chrome from the active palette
                // so light and personality themes retain the same hierarchy.
                let enc_active_border = accent_on;
                let enc_active_fill = with_alpha(enc_active_border, 0.18);
                let enc_hover_fill = with_alpha(enc_active_border, 0.12);
                let enc_btn_base_fill = palette.neutral_overlay(0.04);
                let enc_btn_base_border = palette.neutral_overlay(0.08);
                let enc_input_fill = controls.fill;
                let enc_input_border = controls.border;
                // P11 — `.encryption-card-description` is text_secondary (#a0a0b0), 12px;
                // the 11px `.encryption-mode-sub` / `.encryption-hint` stay text_muted.
                let enc_desc_color = palette.text_secondary;
                // #7 §10 item 8 (2026-06-01) — Tauri renders `var(--color-text-muted)`
                // at FULL opacity (EncryptionCard.css:60,83); pass `text_muted` directly.
                // The prior `with_alpha(.., 0.95)` faded the mode-sub + hint ~5% extra.
                let enc_muted = palette.text_muted;

                // Section label — 设置加密 / Settings Encryption.
                let enc_label = settings_encryption_label_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_label, body) {
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CARD_TITLE,
                        ),
                        enc_label,
                        title_color,
                        15.0,
                        600,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Leading,
                            v: dwrite::VAlign::Center,
                        },
                    )?;
                }
                // Description line (OneDrive sentence) — P11: 12px text_secondary.
                let enc_desc = settings_encryption_desc_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_desc, body) {
                    self.draw_text_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CARD_DESC,
                        ),
                        enc_desc,
                        enc_desc_color,
                        12.0,
                        400,
                        1.0,
                    )?;
                }
                // Current-mode row — 当前模式: <mode label>. Two draws (label + value).
                // P3 — literal ':' after the label (Tauri JSX `{...}:`); built into the
                // reusable `mask_scratch` (cleared before the passphrase mask reuses it)
                // so the colon append stays allocation-free (§10). P8 — the VALUE is bold
                // (weight 700, Tauri `<strong>`). P2 — the value uses the button-title
                // label source so it equals the active button TITLE (e.g. 自定义口令).
                let enc_current =
                    settings_encryption_current_mode_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_current, body) {
                    let current_label = bento_nano_style::t(
                        bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_CURRENT_MODE,
                    );
                    let label_w = (if current_label.is_ascii() {
                        96.0_f32
                    } else {
                        72.0_f32
                    })
                    .min(enc_current.width);
                    let label_part = bento_nano_style::Rect {
                        x: enc_current.x,
                        y: enc_current.y,
                        width: label_w,
                        height: enc_current.height,
                    };
                    // P3 — append ':' to the localized label without a per-frame heap
                    // alloc by composing into the reusable scratch buffer.
                    self.mask_scratch.clear();
                    self.mask_scratch.push_str(current_label);
                    self.mask_scratch.push(':');
                    let label_buf = core::mem::take(&mut self.mask_scratch);
                    // #7 §10 item 3 (2026-06-01) — `.encryption-current` is 13px/400
                    // (EncryptionCard.css:14); the colon-suffixed label half previously
                    // inherited the default 16px no-wrap format. The VALUE half below
                    // already paints at 13/700.
                    let label_result = self.draw_text_no_wrap_with_style(
                        label_buf.as_str(),
                        label_part,
                        label_color,
                        13.0,
                        400,
                        1.0,
                        dwrite::TextAlign::DEFAULT,
                    );
                    self.mask_scratch = label_buf;
                    label_result?;
                    let value_part = bento_nano_style::Rect {
                        x: label_part.right() + 6.0,
                        y: enc_current.y,
                        width: (enc_current.right() - label_part.right() - 6.0).max(0.0),
                        height: enc_current.height,
                    };
                    // P8 — bold value (weight 700, 13px). Uses the button-title source
                    // (P2) so it matches the active mode button's title exactly.
                    self.draw_text_with_style(
                        localized_encryption_mode_button_label(enc_mode),
                        value_part,
                        title_color,
                        13.0,
                        700,
                        1.0,
                    )?;
                }
                // 3-button mode grid — None / DPAPI / Passphrase. Active button gets the
                // accent fill + border; inactive buttons get the neutral chip fill. Each
                // button paints a bold title + an 11px muted sub-label.
                for index in 0..SETTINGS_ENCRYPTION_MODE_COUNT {
                    let btn = settings_encryption_mode_button_rect(
                        viewport,
                        scroll,
                        &backup_flags,
                        index,
                    );
                    if !row_visible(btn, body) {
                        continue;
                    }
                    let this_mode = match index {
                        0 => crate::state::SettingsEncryptionMode::None,
                        1 => crate::state::SettingsEncryptionMode::Dpapi,
                        _ => crate::state::SettingsEncryptionMode::Passphrase,
                    };
                    let is_active = this_mode == enc_mode;
                    let is_hovered = app.is_settings_encryption_mode_hovered(this_mode);
                    // V21-N7 (2026-06-26) — Tauri `.encryption-mode-btn:hover:not(:disabled)`
                    // paints `rgba(96,165,250,0.12)`. Active remains stronger: the
                    // selected button keeps the 0.18 fill even under the pointer.
                    // P5 — ALWAYS fill (base rgba(255,255,255,0.04) / active 96,165,250,0.18)
                    // and ALWAYS stroke a 1px border (base rgba(255,255,255,0.08) / active
                    // #60a5fa). #7 §10 item 6 (2026-06-01) — Tauri `.encryption-mode-btn
                    // .active` only changes the border COLOR (#60a5fa); the WIDTH stays the
                    // base 1px (EncryptionCard.css:32,44-46). The prior 1.5px active stroke
                    // read ~50% heavier than the inactive chips — the visible delta this fixes.
                    self.fill_rounded_rect(
                        btn,
                        settings_encryption_mode_button_fill_color(
                            is_active,
                            is_hovered,
                            enc_btn_base_fill,
                            enc_hover_fill,
                            enc_active_fill,
                        ),
                        btn_radius,
                    )?;
                    if is_active {
                        self.stroke_rounded_rect(btn, enc_active_border, btn_radius, 1.0)?;
                    } else {
                        self.stroke_rounded_rect(btn, enc_btn_base_border, btn_radius, 1.0)?;
                    }
                    // Title (top line) + sub-label (bottom line) stacked inside the btn.
                    let (title_id, sub_id) = match index {
                        0 => (
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_NONE,
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_NONE_SUB,
                        ),
                        1 => (
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_DPAPI,
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_DPAPI_SUB,
                        ),
                        _ => (
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_PASSPHRASE_FULL,
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_MODE_PASSPHRASE_SUB,
                        ),
                    };
                    // #7 §10 item 7 (2026-06-01) — Tauri `.encryption-mode-btn` is
                    // `padding: 10px 12px` with `gap: 4px` (EncryptionCard.css:29,36).
                    // Title sits 12px from the left / 10px from the top; the sub-label
                    // follows 4px below the title. The prior btn.x+6 / btn.y+4 packed the
                    // text too tight to the chip edges. `SETTINGS_ENCRYPTION_BTN_ROW_H`
                    // was bumped 44→52 to fit (10 + 13 + 4 + 11 + 10 ≈ 48 with rounding).
                    let title_rect = bento_nano_style::Rect {
                        x: btn.x + 12.0,
                        y: btn.y + 10.0,
                        width: (btn.width - 24.0).max(0.0),
                        height: 16.0,
                    };
                    // P7 — the title is ALWAYS text_primary (Tauri `.encryption-mode-title`
                    // has `color: inherit`, no active recolor). Activation is conveyed by
                    // the fill + border only. The prior accent-blue active title was the
                    // visible delta this fixes. #7 §10 item 1 — `.encryption-mode-title`
                    // is `font-weight: 600; font-size: 13px` (EncryptionCard.css:53-56);
                    // no explicit line-height on the title (1.0).
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(title_id),
                        title_rect,
                        title_color,
                        13.0,
                        600,
                        1.0,
                        dwrite::TextAlign::DEFAULT,
                    )?;
                    let sub_rect = bento_nano_style::Rect {
                        x: btn.x + 12.0,
                        y: title_rect.bottom() + 4.0,
                        width: (btn.width - 24.0).max(0.0),
                        height: 16.0,
                    };
                    // #7 §10 item 2 — `.encryption-mode-sub` is `font-size: 11px;
                    // line-height: 1.3` at text_muted (EncryptionCard.css:58-62).
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(sub_id),
                        sub_rect,
                        enc_muted,
                        11.0,
                        400,
                        1.3,
                        dwrite::TextAlign::DEFAULT,
                    )?;
                }
                // P4 — passphrase ROW left label cell (口令 / Passphrase). Tauri puts a
                // `<span>` to the LEFT of the input (`justify-content: space-between`);
                // the token (id 238) existed but was never painted. 13px title color.
                let enc_pass_label =
                    settings_encryption_passphrase_label_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_pass_label, body) {
                    let label_text_rect = bento_nano_style::Rect {
                        x: enc_pass_label.x,
                        y: enc_pass_label.y + (enc_pass_label.height - 16.0) * 0.5,
                        width: enc_pass_label.width,
                        height: 16.0,
                    };
                    // #7 §10 item 4 (2026-06-01) — `.encryption-passphrase-row` is
                    // `font-size: 13px` (EncryptionCard.css:64-70); the `<span>` label
                    // inherits it. Previously drawn at the default 16px no-wrap format.
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_LABEL,
                        ),
                        label_text_rect,
                        title_color,
                        13.0,
                        400,
                        1.0,
                        dwrite::TextAlign::DEFAULT,
                    )?;
                }
                // Masked passphrase input box (RIGHT sub-rect of the row — P4). Paints
                // '•' × draft-char-count (or the placeholder when empty + not focused),
                // plus a BLINKING caret bar (P1) at the text end when focused. Never the
                // literal draft. P6 — ALWAYS stroke a 1px base border + fill; focus
                // re-strokes the accent on top.
                let enc_input =
                    settings_encryption_passphrase_input_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_input, body) {
                    self.fill_rounded_rect(enc_input, enc_input_fill, input_box_radius)?;
                    // P6 — base 1px border always; P1/focus — accent re-stroke on top.
                    self.stroke_rounded_rect(enc_input, enc_input_border, input_box_radius, 1.0)?;
                    if enc_passphrase_focused {
                        self.stroke_rounded_rect(
                            enc_input,
                            enc_active_border,
                            input_box_radius,
                            1.0,
                        )?;
                    }
                    // #7 §10 item 5 (2026-06-01) — Tauri input `padding: 6px 10px`
                    // (EncryptionCard.css:78); the L/R inset is 10px (was 12px here).
                    let text_rect = bento_nano_style::Rect {
                        x: enc_input.x + 10.0,
                        y: enc_input.y + (enc_input.height - 16.0) * 0.5,
                        width: (enc_input.width - 20.0).max(0.0),
                        height: 16.0,
                    };
                    if enc_pass_len == 0 && !enc_passphrase_focused {
                        // P16 — placeholder at ~45% of the primary text color (Tauri
                        // ::placeholder default), distinct from the live-text color.
                        // #7 §10 item 5 — input text is `font-size: 12px`
                        // (EncryptionCard.css:79); placeholder shares it.
                        self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_PLACEHOLDER,
                        ),
                        text_rect,
                        with_alpha(palette.text_primary, 0.45),
                        12.0,
                        400,
                        1.0,
                        dwrite::TextAlign::DEFAULT,
                    )?;
                    } else {
                        // Build the mask once into the reusable scratch buffer (cleared,
                        // not freed → allocation-light per §10). U+2022 BULLET.
                        self.mask_scratch.clear();
                        for _ in 0..enc_pass_len {
                            self.mask_scratch.push('\u{2022}');
                        }
                        // P1 — append the caret glyph ONLY on the ON half of the blink
                        // (gated by `caret_on`); on the OFF half it's omitted so the caret
                        // visibly blinks at the Windows ~530ms cadence.
                        if enc_passphrase_focused && caret_on {
                            self.mask_scratch.push('\u{2502}'); // U+2502 BOX DRAWINGS LIGHT VERTICAL
                        }
                        // Clone-free: pass a &str slice of the scratch buffer. The draw
                        // call copies into its own utf16 scratch, so the borrow is short.
                        // #7 §10 item 5 — masked text is the input's 12px/400 (CSS:79).
                        let masked = core::mem::take(&mut self.mask_scratch);
                        let draw_result = self.draw_text_no_wrap_with_style(
                            masked.as_str(),
                            text_rect,
                            title_color,
                            12.0,
                            400,
                            1.0,
                            dwrite::TextAlign::DEFAULT,
                        );
                        self.mask_scratch = masked;
                        draw_result?;
                    }
                }
                // Hint line — never-stored sentence, 11px muted.
                let enc_hint = settings_encryption_hint_rect(viewport, scroll, &backup_flags);
                if row_visible(enc_hint, body) {
                    self.draw_text_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::ENCRYPTION_PASSPHRASE_HINT,
                        ),
                        enc_hint,
                        enc_muted,
                        11.0,
                        400,
                        1.0,
                    )?;
                }
                // Status banner — painted only when a status is set. Error → red,
                // Success → green (Tauri `#f87171` / `#34d399`).
                if let Some(status) = enc_status_snapshot.as_ref() {
                    let enc_status_row =
                        settings_encryption_status_rect(viewport, scroll, &backup_flags);
                    if row_visible(enc_status_row, body) {
                        let (text, color) = match status {
                            crate::state::SettingsBackupStatus::Error(msg) => {
                                (msg.as_str(), with_alpha(palette.accent_red, 0.95))
                            }
                            crate::state::SettingsBackupStatus::Success(msg) => {
                                (msg.as_str(), with_alpha(palette.accent_green, 0.95))
                            }
                        };
                        self.draw_settings_text(text, enc_status_row, color)?;
                    }
                }

                // ── M1h — Plugins §11 section (`SettingsPanel.tsx:709-781`) ──────
                //
                // Sits after the Encryption §10 card in the Tauri body order
                // (…→Backup→**Encryption**→Plugins→footer). M7 (2026-06-01) re-anchored
                // `settings_plugins_label_rect` off the encryption card's status row, so
                // this paint follows the encryption block automatically. Reads the live
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
                    SETTINGS_PLUGINS_ROW_VISIBLE_MAX, settings_plugin_author_rect,
                    settings_plugin_badge_rect, settings_plugin_card_rect,
                    settings_plugin_desc_rect, settings_plugin_empty_row_rect,
                    settings_plugin_name_rect, settings_plugin_status_rect,
                    settings_plugin_toggle_hit_rect, settings_plugin_uninstall_button_rect,
                    settings_plugin_uninstall_cancel_button_rect,
                    settings_plugins_install_button_rect, settings_plugins_label_rect,
                };
                // Snapshot the entries out of the RefCell BEFORE the fallible paint
                // calls so no borrow spans them (mirrors the Backup/Stealth pattern).
                let plugin_entries = app.settings_plugin_entries.borrow().clone();
                let plugin_status_snapshot = app.settings_plugin_status.borrow().clone();
                let plugin_uninstall_confirm = app.settings_plugin_uninstall_confirm.get();
                let plugin_visible = plg::plugin_visible_row_count(&plugin_entries);
                let plugin_flags = backup_flags
                    .with_plugin_rows(plugin_visible)
                    .with_plugin_status(plugin_status_snapshot.is_some());
                // Group title — 插件 / Plugins (reuses SETTINGS_PLUGINS id 36).
                let plugin_label = settings_plugins_label_rect(viewport, scroll, &plugin_flags);
                if row_visible(plugin_label, body) {
                    self.draw_settings_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTINGS_PLUGINS),
                        plugin_label,
                        label_color,
                    )?;
                }
                // Full-width 安装插件... button (neutral chip) → InstallPlugin.
                let plugin_install =
                    settings_plugins_install_button_rect(viewport, scroll, &plugin_flags);
                if row_visible(plugin_install, body) {
                    self.fill_rounded_rect(plugin_install, controls.fill, btn_radius)?;
                    self.stroke_rounded_rect(plugin_install, controls.border, btn_radius, 1.0)?;
                    self.draw_settings_button_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_INSTALL),
                        plugin_install,
                        label_color,
                        13.0,
                        500,
                    )?;
                }
                if let Some(status) = plugin_status_snapshot.as_ref() {
                    let status_row = settings_plugin_status_rect(viewport, scroll, &plugin_flags);
                    if row_visible(status_row, body) {
                        let (text, color) = match status {
                            crate::state::SettingsBackupStatus::Error(message) => {
                                (message.as_str(), with_alpha(palette.accent_red, 0.95))
                            }
                            crate::state::SettingsBackupStatus::Success(message) => {
                                (message.as_str(), with_alpha(palette.accent_green, 0.95))
                            }
                        };
                        self.draw_settings_text_no_wrap(text, status_row, color)?;
                    }
                }
                // plugin-list — N plugin cards or one pluginEmpty placeholder.
                if plg::plugin_list_is_empty(&plugin_entries) {
                    let empty_row = settings_plugin_empty_row_rect(viewport, scroll, &plugin_flags);
                    if row_visible(empty_row, body) {
                        self.draw_text_no_wrap_with_style(
                            bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::PLUGIN_EMPTY),
                            empty_row,
                            palette.text_muted,
                            12.0,
                            400,
                            1.0,
                            dwrite::TextAlign {
                                h: dwrite::HAlign::Center,
                                v: dwrite::VAlign::Center,
                            },
                        )?;
                    }
                } else {
                    for (card_index, plugin) in plugin_entries
                        .iter()
                        .take(SETTINGS_PLUGINS_ROW_VISIBLE_MAX)
                        .enumerate()
                    {
                        let card =
                            settings_plugin_card_rect(viewport, scroll, &plugin_flags, card_index);
                        if !row_visible(card, body) {
                            continue;
                        }
                        // Card surface — raised chip behind the whole card.
                        self.fill_rounded_rect(card, chip_bg, chip_radius)?;
                        self.stroke_rounded_rect(card, chip_border, chip_radius, 1.0)?;
                        // Header — name · v{version} (left), type badge + enable toggle
                        // (right). The header text is formatted once per visible card.
                        let name_rect = settings_plugin_name_rect(card);
                        self.draw_settings_text_no_wrap(
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
                        self.draw_settings_button_text(
                            bento_nano_style::t(plg::plugin_type_label_id(
                                plugin.plugin_type.as_str(),
                            )),
                            badge_rect,
                            with_alpha(badge_accent, 1.0),
                            11.0,
                            600,
                        )?;
                        // Enable toggle — accent when on, neutral track when off →
                        // TogglePlugin(card_index).
                        let toggle_rect = settings_plugin_toggle_hit_rect(card);
                        let toggle_radius =
                            bento_nano_style::BorderRadius::all(toggle_rect.height * 0.5);
                        self.fill_rounded_rect(
                            toggle_rect,
                            if plugin.enabled { accent_on } else { track_off },
                            toggle_radius,
                        )?;
                        self.draw_settings_button_text(
                            if plugin.enabled {
                                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_ON)
                            } else {
                                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_OFF)
                            },
                            toggle_rect,
                            if plugin.enabled {
                                controls.on_accent
                            } else {
                                title_color
                            },
                            11.0,
                            600,
                        )?;
                        // Author line (muted).
                        let author_rect = settings_plugin_author_rect(card);
                        self.draw_settings_text_no_wrap(
                            plugin.author.as_str(),
                            author_rect,
                            with_alpha(palette.text_muted, 0.95),
                        )?;
                        // Description line (muted).
                        let desc_rect = settings_plugin_desc_rect(card);
                        self.draw_settings_text_no_wrap(
                            plugin.description.as_str(),
                            desc_rect,
                            with_alpha(palette.text_muted, 0.95),
                        )?;
                        // Actions — the first destructive click arms an inline
                        // confirmation; no native dialog or intermediate window.
                        let uninstall_btn = settings_plugin_uninstall_button_rect(card);
                        if plugin_uninstall_confirm == Some(card_index) {
                            let cancel_btn = settings_plugin_uninstall_cancel_button_rect(card);
                            let prompt_rect = bento_nano_style::Rect {
                                x: desc_rect.x,
                                y: uninstall_btn.y,
                                width: (cancel_btn.x - desc_rect.x - 8.0).max(0.0),
                                height: uninstall_btn.height,
                            };
                            self.draw_text_no_wrap_with_style(
                                bento_nano_style::t(
                                    bento_nano_style::i18n_zh_cn::ids::PLUGIN_CONFIRM_UNINSTALL,
                                ),
                                prompt_rect,
                                with_alpha(palette.accent_red, 0.95),
                                11.0,
                                500,
                                1.0,
                                dwrite::TextAlign {
                                    h: dwrite::HAlign::Leading,
                                    v: dwrite::VAlign::Center,
                                },
                            )?;
                            self.fill_rounded_rect(cancel_btn, chip_bg, btn_radius)?;
                            self.stroke_rounded_rect(cancel_btn, chip_border, btn_radius, 1.0)?;
                            self.draw_settings_button_text(
                                bento_nano_style::t(
                                    bento_nano_style::i18n_zh_cn::ids::SETTING_CANCEL,
                                ),
                                cancel_btn,
                                label_color,
                                12.0,
                                500,
                            )?;
                            self.fill_rounded_rect(
                                uninstall_btn,
                                with_alpha(palette.accent_red, 0.90),
                                btn_radius,
                            )?;
                            self.draw_settings_button_text(
                                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::BTN_CONFIRM),
                                uninstall_btn,
                                palette.readable_text_on(palette.accent_red),
                                12.0,
                                600,
                            )?;
                        } else {
                            self.fill_rounded_rect(
                                uninstall_btn,
                                with_alpha(palette.accent_red, 0.85),
                                btn_radius,
                            )?;
                            self.draw_settings_button_text(
                                bento_nano_style::t(
                                    bento_nano_style::i18n_zh_cn::ids::PLUGIN_UNINSTALL,
                                ),
                                uninstall_btn,
                                palette.readable_text_on(palette.accent_red),
                                12.0,
                                500,
                            )?;
                        }
                    }
                }

                // ── M6-UI / G3 parity — §3 Appearance inline theme grid (`SettingsPanel.tsx:396-536`) ──
                //
                // G3 parity (2026-06-01): §3 Appearance now flows between §2 Paths and
                // §4 DisplayMode (Tauri body order General → Paths → **Appearance** →
                // DisplayMode → Performance), no longer LAST after Plugins. The geometry
                // helpers (`settings_appearance_label_rect` et al.) re-anchor off the §2
                // 监控值 textarea bottom, so this paint block lands at its new position
                // automatically (paint==hit SSoT) even though it stays here in source
                // order. The grid geometry (group headings + 17 ThemeCards + accent
                // swatch row) is owned by `theme_picker::appearance_layout`; the section
                // anchor + content width come from `settings_panel`. Selecting a card re-skins
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
                    self as tp, AppearanceLayout, BUILTIN_THEMES, SWATCH_BLOCK_RADIUS,
                    SWATCH_INNER_GAP, THEME_CARD_BORDER, THEME_CARD_RADIUS, THEME_GROUP_ORDER,
                };
                // Live theme id (the active card highlight) — borrowed once.
                let active_theme_id = app.active_theme_id.borrow().clone();
                let appearance_hover = app.settings_appearance_hover.get();
                let accent_value = app.settings_accent_editor_value();
                // Group title — 外观 / Appearance.
                let appearance_label =
                    settings_appearance_label_rect(viewport, scroll, &plugin_flags);
                if row_visible(appearance_label, body) {
                    self.draw_settings_group_title(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_APPEARANCE,
                        ),
                        appearance_label,
                        label_color,
                    )?;
                }
                // "选择主题 / Choose Theme" picker label.
                let picker_label =
                    settings_appearance_picker_label_rect(viewport, scroll, &plugin_flags);
                if row_visible(picker_label, body) {
                    self.draw_settings_text(
                        bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::THEME_PICKER_LABEL),
                        picker_label,
                        label_color,
                    )?;
                }
                // Grid layout — body-width-driven, Copy, allocation-free.
                let appearance_origin =
                    settings_appearance_grid_origin(viewport, scroll, &plugin_flags);
                let appearance_inner_w = settings_appearance_inner_width(viewport);
                let appearance: AppearanceLayout =
                    tp::appearance_layout(appearance_origin, appearance_inner_w);
                // surface_subtle = rgba(white, 0.04) card bg (live theme). Active card
                // overrides to accent-blue@0.10 + a 2-DIP accent-blue rounded border.
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
                    let is_hovered = appearance_hover == Some(tp::AppearanceHit::Card(preset.id));
                    let selection_progress =
                        app.theme_card_selection_progress_at(preset.id, is_active, settings_now_ms);
                    let card_chrome =
                        settings_theme_card_chrome(palette, selection_progress, is_hovered);
                    // Card surface.
                    self.fill_rounded_rect(card, card_chrome.fill, card_radius)?;
                    // Active card border — 2-DIP accent-blue. Tauri's CSS `border` is a
                    // fully-inset border-box; D2D strokes centred on the geometric edge,
                    // so the rect is inset by half the stroke width (1 DIP) on all sides
                    // and the radius shrinks to stay concentric — no bleed past the card.
                    if let Some(border_color) = card_chrome.border {
                        let inset = THEME_CARD_BORDER * 0.5;
                        let border_rect = bento_nano_style::Rect {
                            x: card.x + inset,
                            y: card.y + inset,
                            width: (card.width - THEME_CARD_BORDER).max(0.0),
                            height: (card.height - THEME_CARD_BORDER).max(0.0),
                        };
                        let border_radius = bento_nano_style::BorderRadius::all(
                            (THEME_CARD_RADIUS - inset).max(0.0),
                        );
                        self.stroke_rounded_rect(
                            border_rect,
                            border_color,
                            border_radius,
                            THEME_CARD_BORDER,
                        )?;
                    }
                    // 40×40 swatch block — 4 quadrant fills (3-DIP gutter == gap:3px).
                    let block = appearance.swatch_blocks[i];
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
                        [true, false, false, false], // 0 = TL
                        [false, true, false, false], // 1 = TR
                        [false, false, false, true], // 2 = BL
                        [false, false, true, false], // 3 = BR
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
                    // #1 step 13 (2026-06-02) — was the lone `draw_text_centered` helper;
                    // now folded into the unified styled path with explicit center/center.
                    self.draw_text_no_wrap_with_style(
                        bento_nano_style::t(preset.name_id),
                        label_rect,
                        palette.text_secondary,
                        10.0,
                        400,
                        1.0,
                        dwrite::TextAlign {
                            h: dwrite::HAlign::Center,
                            v: dwrite::VAlign::Center,
                        },
                    )?;
                }
                // Accent row — Tauri's single compact colour input, backed by
                // Nano's existing native ChooseColorW producer.
                if row_visible(appearance.accent_row, body) {
                    let accent_picker = appearance.accent_picker;
                    let accent_label_rect = bento_nano_style::Rect {
                        x: appearance.accent_row.x,
                        y: appearance.accent_row.y,
                        width: (accent_picker.x - appearance.accent_row.x - 8.0).max(0.0),
                        height: appearance.accent_row.height,
                    };
                    self.draw_settings_text_no_wrap(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_ACCENT_COLOR,
                        ),
                        accent_label_rect,
                        label_color,
                    )?;
                    let accent_picker_hovered =
                        appearance_hover == Some(tp::AppearanceHit::AccentPicker);
                    let accent_picker_radius = bento_nano_style::BorderRadius::all(6.0);
                    self.fill_rounded_rect(
                        accent_picker,
                        if accent_picker_hovered {
                            with_alpha(accent_on, 0.10)
                        } else {
                            chip_bg
                        },
                        accent_picker_radius,
                    )?;
                    self.stroke_rounded_rect(
                        accent_picker,
                        if accent_picker_hovered {
                            with_alpha(accent_on, 0.72)
                        } else {
                            chip_border
                        },
                        accent_picker_radius,
                        if accent_picker_hovered { 1.5 } else { 1.0 },
                    )?;
                    let preview = bento_nano_style::Rect {
                        x: accent_picker.x + 3.0,
                        y: accent_picker.y + 3.0,
                        width: accent_picker.width - 6.0,
                        height: accent_picker.height - 6.0,
                    };
                    let preview_color = parse_hex_color(accent_value.as_str())
                        .unwrap_or_else(|| with_alpha(palette.text_muted, 0.35));
                    self.fill_rounded_rect(
                        preview,
                        preview_color,
                        bento_nano_style::BorderRadius::all(4.0),
                    )?;
                }

                // ── §4 Zone Display Mode ──
                // Tauri `SettingsPanel.tsx:538-598` uses a left explanatory
                // label/hint and a right-aligned vertical stack of three full
                // option cards. The shared settings-panel rects also drive
                // hit-testing, so the complete card remains clickable.
                let display_mode_label =
                    crate::settings_panel::settings_display_mode_label_rect(viewport, scroll);
                if row_visible(display_mode_label, body) {
                    self.draw_settings_group_title(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_GROUP_DISPLAY_MODE,
                        ),
                        display_mode_label,
                        palette.text_muted,
                    )?;
                }
                let picker_row = settings_zone_display_mode_picker_row_rect(viewport, scroll);
                if row_visible(picker_row, body) {
                    let copy_label = settings_display_mode_copy_label_rect(viewport, scroll);
                    self.draw_settings_text(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_DISPLAY_MODE_LABEL,
                        ),
                        copy_label,
                        label_color,
                    )?;
                    let hint = settings_display_mode_hint_rect(viewport, scroll);
                    self.draw_text_with_style(
                        bento_nano_style::t(
                            bento_nano_style::i18n_zh_cn::ids::SETTINGS_DISPLAY_MODE_HINT,
                        ),
                        hint,
                        palette.text_muted,
                        11.0,
                        400,
                        1.25,
                    )?;
                    let modes = [
                        ZoneDisplayMode::Hover,
                        ZoneDisplayMode::Always,
                        ZoneDisplayMode::Click,
                    ];
                    let current = app.zone_display_mode.get();
                    let radius_outer = BorderRadius::all(SETTINGS_RADIO_OUTER_D * 0.5);
                    let radius_inner = BorderRadius::all(SETTINGS_RADIO_INNER_D * 0.5);
                    let option_radius = BorderRadius::all(8.0);
                    for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
                        let mode = modes[index as usize];
                        let option = crate::settings_panel::settings_zone_display_mode_radio_rect(
                            viewport, scroll, index,
                        );
                        if mode == current {
                            self.fill_rounded_rect(
                                option,
                                with_alpha(accent_on, 0.10),
                                option_radius,
                            )?;
                            self.stroke_rounded_rect(
                                option,
                                with_alpha(accent_on, 0.35),
                                option_radius,
                                1.0,
                            )?;
                        }
                        let outer =
                            settings_zone_display_mode_radio_outer_rect(viewport, scroll, index);
                        let ring_color = if mode == current {
                            accent_on
                        } else {
                            chip_border
                        };
                        self.stroke_rounded_rect(outer, ring_color, radius_outer, 1.0)?;
                        if mode == current {
                            let inner = settings_zone_display_mode_radio_inner_rect(
                                viewport, scroll, index,
                            );
                            self.fill_rounded_rect(inner, accent_on, radius_inner)?;
                        }
                        // Full Tauri option copy via StringId 77/78/79.
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
                        let label =
                            settings_zone_display_mode_radio_label_rect(viewport, scroll, index);
                        self.draw_text_no_wrap_with_style(
                            bento_nano_style::t(label_id),
                            label,
                            title_color,
                            crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
                            crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
                            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
                            dwrite::TextAlign::DEFAULT,
                        )?;
                    }
                }

                // Native D2D equivalent of Tauri's 4px WebKit scrollbar. The
                // thumb uses the exact live body flags so its size and position
                // remain truthful when source/backup/plugin rows change.
                let scrollbar_flags = plugin_flags.with_source_rows(source_count);
                let content_h = settings_body_content_height(viewport, &scrollbar_flags);
                if let Some(thumb) =
                    settings_scrollbar_thumb_rect(viewport, content_h, app.scroll_offset_y.get())
                {
                    self.fill_rounded_rect(
                        thumb,
                        with_alpha(palette.text_primary, 0.24),
                        BorderRadius::all(SETTINGS_SCROLLBAR_W * 0.5),
                    )?;
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
            let footer_hairline = bento_nano_style::Rect {
                x: footer.x,
                y: footer.y,
                width: footer.width,
                height: 1.0,
            };
            self.fill_rounded_rect(footer_hairline, divider_color, BorderRadius::ZERO)?;
            let cancel_btn = settings_cancel_button_rect(viewport);
            if let Some(error) = app.settings_save_error.borrow().as_ref() {
                self.draw_text_with_style(
                    error.as_str(),
                    bento_nano_style::Rect {
                        x: footer.x + SETTINGS_ROW_PAD_X,
                        y: footer.y + 8.0,
                        width: (cancel_btn.x - footer.x - SETTINGS_ROW_PAD_X * 2.0).max(0.0),
                        height: footer.height - 16.0,
                    },
                    with_alpha(palette.accent_red, 0.98),
                    10.5,
                    500,
                    1.25,
                )?;
            }
            self.fill_rounded_rect(cancel_btn, controls.fill, btn_radius)?;
            self.stroke_rounded_rect(cancel_btn, controls.border, btn_radius, 1.0)?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_CANCEL),
                cancel_btn,
                label_color,
                13.0,
                500,
            )?;
            // M1a 2026-05-29 — Save dims to ~0.4 alpha when no toggle has been
            // touched since the panel opened, mirroring Tauri `disabled={!dirty()}`
            // at `SettingsPanel.tsx:799`. The hit-tester treats the dimmed button
            // as a no-op (`SaveSettings` dispatch arm short-circuits when
            // `!settings_dirty`); Cancel stays always-active.
            let save_btn = settings_save_button_rect(viewport);
            let dirty = app.settings_dirty.get();
            let save_fill = if dirty {
                accent_on
            } else {
                controls.disabled_fill
            };
            let save_text = if dirty {
                controls.on_accent
            } else {
                controls.disabled_text
            };
            self.fill_rounded_rect(save_btn, save_fill, btn_radius)?;
            self.stroke_rounded_rect(
                save_btn,
                if dirty {
                    accent_on
                } else {
                    controls.disabled_border
                },
                btn_radius,
                1.0,
            )?;
            self.draw_settings_button_text(
                bento_nano_style::t(bento_nano_style::i18n_zh_cn::ids::SETTING_SAVE),
                save_btn,
                save_text,
                13.0,
                500,
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
        })();
        if open_transform_active {
            self.set_logical_transform_override(None)?;
        }
        settings_paint
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

    /// Decode one compiled-in About image once per renderer/device generation
    /// and draw it without a filesystem or network dependency.
    fn draw_embedded_about_image(
        &mut self,
        cache_key: &str,
        bytes: &[u8],
        role: &'static str,
        rect: bento_nano_style::Rect,
    ) -> Result<(), RenderError> {
        if rect.width <= 0.0 || rect.height <= 0.0 || self.image_file_failures.contains(cache_key) {
            return Ok(());
        }

        if !self.image_file_bitmaps.contains_key(cache_key) {
            let Some(surface) = self.surface.as_ref() else {
                return Ok(());
            };
            match d2d::bitmap_from_image_bytes(&surface.ctx, bytes) {
                Ok(bitmap) => {
                    let _ = self.image_file_bitmaps.insert(cache_key.to_owned(), bitmap);
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::render::about",
                        image_role = role,
                        error = %error,
                        "failed to decode compiled-in About image"
                    );
                    let _ = self.image_file_failures.insert(cache_key.to_owned());
                    return Ok(());
                }
            }
        }

        let Some(bitmap) = self.image_file_bitmaps.get(cache_key).cloned() else {
            return Ok(());
        };
        let destination = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        d2d::draw_bitmap(&surface.ctx, &bitmap, destination, 1.0)?;
        Ok(())
    }

    fn draw_about_app_icon(&mut self, rect: bento_nano_style::Rect) -> Result<(), RenderError> {
        self.draw_embedded_about_image(
            "embedded:about-app-icon",
            include_bytes!("../assets/app-icon.png"),
            "app-icon",
            rect,
        )
    }

    fn draw_about_avatar(&mut self, rect: bento_nano_style::Rect) -> Result<(), RenderError> {
        self.draw_embedded_about_image(
            "embedded:about-author-avatar",
            include_bytes!("../assets/about-avatar.png"),
            "author-avatar",
            rect,
        )
    }

    /// Draw the selected-stack About window as a complete native product
    /// surface: identity, author, version, stack, design principles and a real
    /// GitHub action. The opaque fallback intentionally avoids the old fuzzy
    /// shadow/transparent halo on hosts where acrylic is unavailable.
    fn draw_about_panel(&mut self, app: &AppState) -> Result<(), RenderError> {
        use crate::business::about;

        let palette = app.active_theme_palette();
        let radius = app.active_theme_radius();
        let viewport = app.viewport;
        let panel = about::panel_rect(viewport);
        let panel_bg = with_alpha(palette.surface, 1.0);
        let card_bg = with_alpha(palette.surface_alt, 0.96);
        let button_bg = with_alpha(palette.surface_alt, 0.98);
        let border = with_alpha(palette.border, 0.78);
        let accent_border = with_alpha(palette.accent, 0.58);
        let title = with_alpha(palette.text, 1.0);
        let body = with_alpha(palette.text, 0.94);
        let muted = with_alpha(palette.text_muted, 0.94);
        let accent = with_alpha(palette.accent, 1.0);

        self.fill_rounded_rect(panel, panel_bg, radius.xl)?;
        self.stroke_rounded_rect(panel, border, radius.xl, 1.0)?;

        let app_icon_frame = about::app_icon_rect(viewport);
        self.fill_rounded_rect(app_icon_frame, card_bg, radius.lg)?;
        self.stroke_rounded_rect(app_icon_frame, accent_border, radius.lg, 1.0)?;
        self.draw_about_app_icon(bento_nano_style::Rect {
            x: app_icon_frame.x + 6.0,
            y: app_icon_frame.y + 6.0,
            width: app_icon_frame.width - 12.0,
            height: app_icon_frame.height - 12.0,
        })?;

        let identity_x = app_icon_frame.right() + 18.0;
        let identity_w = (panel.right() - identity_x - 76.0).max(0.0);
        self.draw_text_no_wrap_with_style(
            "BentoDesk",
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 30.0,
                width: identity_w,
                height: 34.0,
            },
            title,
            26.0,
            700,
            1.2,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        self.draw_text_no_wrap_with_style(
            "轻量、原生、专注的 Windows 桌面整理器",
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 68.0,
                width: identity_w,
                height: 22.0,
            },
            body,
            14.0,
            500,
            1.35,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            about::format_version().as_str(),
            bento_nano_style::Rect {
                x: identity_x,
                y: panel.y + 98.0,
                width: 132.0,
                height: 22.0,
            },
            accent,
            11.0,
            600,
            1.3,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;

        let content_x = panel.x + about::CONTENT_PADDING;
        let content_w = panel.width - about::CONTENT_PADDING * 2.0;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 132.0,
                width: content_w,
                height: 1.0,
            },
            with_alpha(palette.border, 0.45),
            BorderRadius::ZERO,
        )?;
        self.draw_text_no_wrap_with_style(
            "为专注而整理",
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 151.0,
                width: content_w,
                height: 26.0,
            },
            title,
            18.0,
            650,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_with_style(
            "用原生技术重新组织桌面空间，让文件、快捷方式和工作流清爽、可控；保留成熟版本的体验，同时移除 WebView 运行时负担。",
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 183.0,
                width: content_w,
                height: 38.0,
            },
            body,
            12.5,
            400,
            1.55,
        )?;

        let card_gap = 12.0;
        let card_w = (content_w - card_gap) * 0.5;
        for (index, (heading, detail, icon)) in [
            ("原生运行时", "Rust · Win32 · Direct2D", "code"),
            ("开源许可证", about::LICENSE_NAME, "copy"),
        ]
        .into_iter()
        .enumerate()
        {
            let card = bento_nano_style::Rect {
                x: content_x + index as f32 * (card_w + card_gap),
                y: panel.y + 232.0,
                width: card_w,
                height: 82.0,
            };
            self.fill_rounded_rect(card, card_bg, radius.md)?;
            self.stroke_rounded_rect(card, border, radius.md, 1.0)?;
            self.draw_icon_glyph(
                icon,
                bento_nano_style::Rect {
                    x: card.x + 15.0,
                    y: card.y + 16.0,
                    width: 17.0,
                    height: 17.0,
                },
                accent,
            )?;
            self.draw_text_no_wrap_with_style(
                heading,
                bento_nano_style::Rect {
                    x: card.x + 42.0,
                    y: card.y + 14.0,
                    width: card.width - 57.0,
                    height: 20.0,
                },
                title,
                13.0,
                600,
                1.3,
                dwrite::TextAlign::DEFAULT,
            )?;
            self.draw_text_no_wrap_with_style(
                detail,
                bento_nano_style::Rect {
                    x: card.x + 16.0,
                    y: card.y + 46.0,
                    width: card.width - 32.0,
                    height: 18.0,
                },
                muted,
                11.5,
                450,
                1.3,
                dwrite::TextAlign::DEFAULT,
            )?;
        }

        let project = about::project_button_rect(viewport);
        self.fill_rounded_rect(project, button_bg, radius.md)?;
        self.stroke_rounded_rect(project, accent_border, radius.md, 1.0)?;
        self.draw_icon_glyph(
            "external_link",
            bento_nano_style::Rect {
                x: project.x + 16.0,
                y: project.y + 16.0,
                width: 18.0,
                height: 18.0,
            },
            accent,
        )?;
        self.draw_text_no_wrap_with_style(
            "项目源代码",
            bento_nano_style::Rect {
                x: project.x + 46.0,
                y: project.y + 6.0,
                width: 116.0,
                height: 18.0,
            },
            title,
            12.5,
            600,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            about::PROJECT_URL,
            bento_nano_style::Rect {
                x: project.x + 46.0,
                y: project.y + 25.0,
                width: project.width - 94.0,
                height: 15.0,
            },
            muted,
            10.0,
            400,
            1.25,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_icon_glyph(
            "arrow_right",
            centered_square_rect(
                bento_nano_style::Rect {
                    x: project.right() - 42.0,
                    y: project.y,
                    width: 42.0,
                    height: project.height,
                },
                14.0,
            ),
            muted,
        )?;

        let author = about::author_button_rect(viewport);
        self.fill_rounded_rect(author, card_bg, radius.md)?;
        self.stroke_rounded_rect(author, border, radius.md, 1.0)?;
        let avatar = about::author_avatar_rect(viewport);
        self.fill_rounded_rect(avatar, with_alpha(palette.surface, 1.0), radius.md)?;
        self.stroke_rounded_rect(avatar, border, radius.md, 1.0)?;
        self.draw_about_avatar(bento_nano_style::Rect {
            x: avatar.x + 2.0,
            y: avatar.y + 2.0,
            width: avatar.width - 4.0,
            height: avatar.height - 4.0,
        })?;
        self.draw_text_no_wrap_with_style(
            format!("作者 · {}", about::AUTHOR).as_str(),
            bento_nano_style::Rect {
                x: avatar.right() + 13.0,
                y: author.y + 10.0,
                width: 180.0,
                height: 20.0,
            },
            title,
            12.5,
            600,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_text_no_wrap_with_style(
            format!("GitHub {}", about::GITHUB_HANDLE).as_str(),
            bento_nano_style::Rect {
                x: avatar.right() + 13.0,
                y: author.y + 33.0,
                width: author.width - avatar.width - 72.0,
                height: 18.0,
            },
            muted,
            10.5,
            400,
            1.3,
            dwrite::TextAlign::DEFAULT,
        )?;
        self.draw_icon_glyph(
            "external_link",
            centered_square_rect(
                bento_nano_style::Rect {
                    x: author.right() - 42.0,
                    y: author.y,
                    width: 42.0,
                    height: author.height,
                },
                14.0,
            ),
            muted,
        )?;

        self.draw_text_no_wrap_with_style(
            format!("{} · {}", about::LICENSE_SUMMARY_ZH, about::LICENSE_NAME).as_str(),
            bento_nano_style::Rect {
                x: content_x,
                y: panel.y + 475.0,
                width: content_w,
                height: 18.0,
            },
            muted,
            9.75,
            400,
            1.25,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;

        let close = about::close_button_rect(viewport);
        self.fill_rounded_rect(close, card_bg, radius.md)?;
        self.stroke_rounded_rect(close, border, radius.md, 1.0)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 14.0), title)?;
        Ok(())
    }

    fn draw_context_menu_row(
        &mut self,
        row: &popover::ContextMenuRow,
        rect: bento_nano_style::Rect,
        hovered: bool,
        palette: bento_nano_style::tokens::PaletteTauri,
        radius: f32,
    ) -> Result<(), RenderError> {
        if row.kind == popover::ContextMenuRowKind::Separator {
            let line = bento_nano_style::Rect {
                x: rect.x + 11.0,
                y: rect.y + (rect.height - 1.0) * 0.5,
                width: (rect.width - 22.0).max(0.0),
                height: 1.0,
            };
            self.fill_rounded_rect(
                line,
                with_alpha(palette.border_expanded, 0.22),
                BorderRadius::all(0.5),
            )?;
            return Ok(());
        }

        let row_body = bento_nano_style::Rect {
            x: rect.x + 5.0,
            y: rect.y + 1.0,
            width: (rect.width - 10.0).max(0.0),
            height: (rect.height - 2.0).max(0.0),
        };
        if hovered {
            let hover = if row.danger {
                with_alpha(palette.accent_red, 0.14)
            } else {
                // `surface_hover` already carries the theme's intentionally
                // subtle alpha. Replacing it with 0.92 turns light RGB tokens
                // into an opaque white bar and destroys label contrast.
                palette.surface_hover
            };
            self.fill_rounded_rect(row_body, hover, BorderRadius::all(radius))?;
        }

        let foreground = if row.danger {
            palette.accent_red
        } else {
            palette.text_primary
        };
        if let Some(icon) = row.icon {
            let icon_rect = bento_nano_style::Rect {
                x: rect.x + 12.0,
                y: rect.y + (rect.height - popover::CONTEXT_MENU_ICON_SIZE) * 0.5,
                width: popover::CONTEXT_MENU_ICON_SIZE,
                height: popover::CONTEXT_MENU_ICON_SIZE,
            };
            self.draw_icon_glyph(
                icon.as_str(),
                icon_rect,
                with_alpha(foreground, if hovered { 1.0 } else { 0.78 }),
            )?;
        }

        let arrow_reserve = if row.kind == popover::ContextMenuRowKind::Submenu {
            24.0
        } else {
            0.0
        };
        let label_rect = bento_nano_style::Rect {
            x: rect.x + 38.0,
            y: rect.y,
            width: (rect.width - 38.0 - 12.0 - arrow_reserve).max(0.0),
            height: rect.height,
        };
        self.draw_text_no_wrap_with_style(
            row.label.as_str(),
            label_rect,
            foreground,
            12.25,
            if hovered { 550 } else { 450 },
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if row.kind == popover::ContextMenuRowKind::Submenu {
            let arrow = bento_nano_style::Rect {
                x: rect.right() - 22.0,
                y: rect.y + (rect.height - 12.0) * 0.5,
                width: 12.0,
                height: 12.0,
            };
            self.draw_icon_glyph(
                IconKind::ArrowRight.as_str(),
                arrow,
                with_alpha(palette.text_muted, 0.90),
            )?;
        }
        Ok(())
    }

    fn draw_context_menu_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let session = app.active_context_menu.borrow();
        let Some(session) = session.as_ref() else {
            return Ok(());
        };
        let palette = app.active_theme_tauri();
        let theme_radius = app.active_theme_radius_tauri();
        let card_radius = theme_radius.card.max(10.0);
        let row_radius = (card_radius - 2.0).max(4.0);
        // Keep the card dense enough for legibility without turning it into a
        // hard black rectangle floating above the desktop. The DComp surface
        // preserves this alpha, matching the glassier Tauri menu treatment.
        let surface = with_alpha(palette.surface_expanded, 0.96);

        for column in [
            popover::ContextMenuColumn::Main,
            popover::ContextMenuColumn::Submenu,
        ] {
            let Some(card) = popover::context_menu_card_rect(session, column) else {
                continue;
            };
            let shadow = bento_nano_style::Rect {
                x: card.x + 1.0,
                y: card.y + 3.0,
                width: card.width,
                height: card.height,
            };
            self.fill_rounded_rect(
                shadow,
                Color::rgba(0.0, 0.0, 0.0, 0.14),
                BorderRadius::all(card_radius + 1.0),
            )?;
            self.fill_rounded_rect(card, surface, BorderRadius::all(card_radius))?;
            self.stroke_rounded_rect(
                card,
                with_alpha(palette.border_expanded, 0.36),
                BorderRadius::all(card_radius),
                1.0,
            )?;
        }

        for row_index in 0..session.main_rows.len() {
            let hit = popover::ContextMenuHit {
                column: popover::ContextMenuColumn::Main,
                row: row_index,
            };
            if let Some(rect) = popover::context_menu_row_rect(session, hit) {
                self.draw_context_menu_row(
                    &session.main_rows[row_index],
                    rect,
                    session.hovered == Some(hit),
                    palette,
                    row_radius,
                )?;
            }
        }

        if session.submenu_open {
            let range = session.visible_submenu_range();
            for row_index in range.clone() {
                let hit = popover::ContextMenuHit {
                    column: popover::ContextMenuColumn::Submenu,
                    row: row_index,
                };
                if let Some(rect) = popover::context_menu_row_rect(session, hit) {
                    self.draw_context_menu_row(
                        &session.submenu_rows[row_index],
                        rect,
                        session.hovered == Some(hit),
                        palette,
                        row_radius,
                    )?;
                }
            }
            if let Some(card) =
                popover::context_menu_card_rect(session, popover::ContextMenuColumn::Submenu)
            {
                let max_start = session
                    .submenu_rows
                    .len()
                    .saturating_sub(popover::CONTEXT_MENU_MAX_SUBMENU_ROWS);
                if session.submenu_scroll > 0 {
                    self.fill_rounded_rect(
                        bento_nano_style::Rect {
                            x: card.right() - 4.0,
                            y: card.y + 8.0,
                            width: 2.0,
                            height: 12.0,
                        },
                        palette.accent_blue,
                        BorderRadius::all(1.0),
                    )?;
                }
                if session.submenu_scroll < max_start {
                    self.fill_rounded_rect(
                        bento_nano_style::Rect {
                            x: card.right() - 4.0,
                            y: card.bottom() - 20.0,
                            width: 2.0,
                            height: 12.0,
                        },
                        palette.accent_blue,
                        BorderRadius::all(1.0),
                    )?;
                }
            }
        }
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
        let tauri_palette = app.active_theme_tauri();
        let bar = bar.with_tauri_tokens(
            tauri_palette,
            // minibar radius is global chrome (same for every theme, design §1.2).
            app.active_theme_radius_tauri(),
            style_tokens::SPACING,
        );
        let viewport = app.viewport;
        let panel = minibar::minibar_panel_rect(viewport);
        self.fill_rounded_rect_vertical_gradient(
            panel,
            tauri_palette.minibar_gradient_top,
            tauri_palette.minibar_gradient_bottom,
            bar.border_radius,
        )?;

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
                self.draw_text(
                    if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                        "区域中暂无项目"
                    } else {
                        "Empty zone"
                    },
                    label_rect,
                    bar.unpin_button.tint,
                )?;
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
                        // bitmap first (mirrors `draw_item_card`). Only when
                        // the cache misses / decode fails do we fall back to
                        // the extension-derived selected-stack line-art glyph.
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
                        if !self.draw_item_bitmap(item.icon_hash.as_ref(), icon_rect, 1.0)? {
                            let kind = item_icon::fallback_icon_kind_for_item(
                                item.icon_hash.as_ref(),
                                item.path.as_ref(),
                            );
                            self.draw_icon_glyph(kind.as_str(), icon_rect, bar.unpin_button.tint)?;
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
        use crate::business::zone_editor::{ACCENT_PALETTE, CapsuleShapeChoice, CapsuleSizeChoice};

        let tauri_palette = app.active_theme_tauri();
        let chrome = zone_editor_geometry::ZoneEditorChrome::from_tauri_tokens(
            tauri_palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = zone_editor_geometry::zone_editor_panel(viewport);
        // ZoneEditor is a compact top-level native dialog, not a wallpaper
        // surface. Its card must remain opaque: preserving the theme token's
        // translucent alpha here made desktop icons and labels legible through
        // every form row, visually resembling a broken browser overlay.
        self.fill_rounded_rect(
            panel,
            with_alpha(chrome.panel_background, 1.0),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
        )?;
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: 190.0_f32.min(panel.width - 90.0),
            height: 28.0,
        };
        // M6c — zone editor panel title (`h2`).
        self.draw_text_chromatic_title(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                "编辑区域"
            } else {
                "Edit zone"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = zone_editor_geometry::zone_editor_close_rect(viewport);
        self.fill_rounded_rect(
            close_rect,
            with_alpha(chrome.body_color, 0.05),
            chrome.row_radius,
        )?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            chrome.muted_color,
        )?;
        let header = zone_editor_geometry::zone_editor_header_rect(viewport);
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: header.x + 1.0,
                y: header.bottom() - 1.0,
                width: (header.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;

        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        let session = app.zone_editor.borrow();
        let label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 54.0,
            width: panel.width - 36.0,
            height: 14.0,
        };
        self.draw_text(
            if zh { "区域名称" } else { "Zone name" },
            label_rect,
            chrome.muted_color,
        )?;

        let input_rect = zone_editor_geometry::zone_editor_name_input_rect(viewport);
        self.fill_rounded_rect(input_rect, chrome.input_background, chrome.input_radius)?;
        if session.is_some() {
            self.stroke_rounded_rect(input_rect, chrome.accent_color, chrome.input_radius, 1.5)?;
        }

        let selected_size = session
            .as_ref()
            .map(|entry| CapsuleSizeChoice::parse(entry.draft_capsule_size.as_str()))
            .unwrap_or_default();
        let selected_shape = session
            .as_ref()
            .map(|entry| CapsuleShapeChoice::parse(entry.draft_capsule_shape.as_str()))
            .unwrap_or_default();
        let draft = session
            .as_ref()
            .map(|s| s.draft_name.as_str())
            .unwrap_or(if zh {
                "尚未选择区域"
            } else {
                "No zone selected"
            });
        let draft_rect = inset_rect(input_rect, 10.0);
        self.draw_text_no_wrap_with_style(
            draft,
            draft_rect,
            chrome.body_color,
            14.0,
            500,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;

        let icon_chip_rect = zone_editor_geometry::zone_editor_icon_rect(viewport);
        let icon_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: icon_chip_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "图标" } else { "Icon" },
            icon_label_rect,
            chrome.muted_color,
        )?;
        self.fill_rounded_rect(icon_chip_rect, chrome.input_background, chrome.row_radius)?;
        let icon_value_rect = bento_nano_style::Rect {
            x: icon_chip_rect.x + 10.0,
            y: icon_chip_rect.y + 4.0,
            width: icon_chip_rect.width - 20.0,
            height: icon_chip_rect.height - 8.0,
        };
        let icon_value = session
            .as_ref()
            .map(|s| s.draft_icon.as_str())
            .unwrap_or("folder");
        self.draw_icon_glyph(
            icon_value,
            bento_nano_style::Rect {
                x: icon_chip_rect.x + 8.0,
                y: icon_chip_rect.y + 4.0,
                width: 18.0,
                height: 18.0,
            },
            chrome.body_color,
        )?;
        self.draw_text(
            localized_icon_wire_label(icon_value, zh),
            bento_nano_style::Rect {
                x: icon_value_rect.x + 24.0,
                width: (icon_value_rect.width - 24.0).max(0.0),
                ..icon_value_rect
            },
            chrome.body_color,
        )?;

        let accent_row_rect = zone_editor_geometry::zone_editor_accent_rect(viewport);
        let accent_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: accent_row_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "强调色" } else { "Accent" },
            accent_label_rect,
            chrome.muted_color,
        )?;
        let selected_accent = session
            .as_ref()
            .and_then(|entry| entry.draft_accent_color.as_deref());
        let custom_selected = selected_accent.is_some_and(|hex| !ACCENT_PALETTE.contains(&hex));
        for index in 0..(ACCENT_PALETTE.len() + 2) {
            let Some(visual) =
                zone_editor_geometry::zone_editor_accent_option_visual_rect(viewport, index)
            else {
                continue;
            };
            let selected = if index == 0 {
                selected_accent.is_none()
            } else if index <= ACCENT_PALETTE.len() {
                selected_accent == Some(ACCENT_PALETTE[index - 1])
            } else {
                custom_selected
            };
            let border = if selected {
                chrome.accent_color
            } else {
                with_alpha(chrome.body_color, 0.16)
            };
            self.fill_rounded_rect(visual, border, chrome.swatch_radius)?;
            let inner = inset_rect(visual, 2.0);
            if index == 0 {
                self.fill_rounded_rect(inner, chrome.input_background, chrome.swatch_inner_radius)?;
                self.draw_text_no_wrap_with_style(
                    "×",
                    inner,
                    chrome.muted_color,
                    12.0,
                    500,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            } else if index <= ACCENT_PALETTE.len() {
                if let Some(color) = parse_hex_color(ACCENT_PALETTE[index - 1]) {
                    self.fill_rounded_rect(inner, color, chrome.swatch_inner_radius)?;
                }
            } else {
                self.fill_rounded_rect(inner, chrome.input_background, chrome.swatch_inner_radius)?;
                self.draw_text_no_wrap_with_style(
                    "+",
                    inner,
                    chrome.body_color,
                    12.0,
                    600,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            }
        }

        let grid_value_rect = zone_editor_geometry::zone_editor_grid_rect(viewport);
        let grid_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: grid_value_rect.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "网格列数" } else { "Grid" },
            grid_label_rect,
            chrome.muted_color,
        )?;
        let selected_columns = session
            .as_ref()
            .map(|entry| entry.draft_grid_columns)
            .unwrap_or(4);
        for columns in crate::business::zone_editor::GRID_COLUMNS_MIN
            ..=crate::business::zone_editor::GRID_COLUMNS_MAX
        {
            let Some(option) =
                zone_editor_geometry::zone_editor_grid_option_rect(viewport, columns)
            else {
                continue;
            };
            let selected = columns == selected_columns;
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                chrome.row_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    chrome.row_radius,
                    1.0,
                )?;
            }
            self.draw_text_no_wrap_with_style(
                grid_columns_label(columns, zh),
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let capsule_size_row = zone_editor_geometry::zone_editor_capsule_size_rect(viewport);
        let capsule_size_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: capsule_size_row.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "宽度" } else { "Width" },
            capsule_size_label_rect,
            chrome.muted_color,
        )?;
        for (index, size) in CapsuleSizeChoice::ALL.iter().copied().enumerate() {
            let Some(option) =
                zone_editor_geometry::zone_editor_capsule_size_option_rect(viewport, index)
            else {
                continue;
            };
            let selected = size == selected_size;
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                chrome.row_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    chrome.row_radius,
                    1.0,
                )?;
            }
            let label = match (zh, size) {
                (true, CapsuleSizeChoice::Small) => "小 · 120",
                (true, CapsuleSizeChoice::Medium) => "中 · 160",
                (true, CapsuleSizeChoice::Large) => "大 · 200",
                (false, CapsuleSizeChoice::Small) => "Small · 120",
                (false, CapsuleSizeChoice::Medium) => "Medium · 160",
                (false, CapsuleSizeChoice::Large) => "Large · 200",
            };
            self.draw_text_no_wrap_with_style(
                label,
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let capsule_shape_row = zone_editor_geometry::zone_editor_capsule_shape_rect(viewport);
        let capsule_shape_label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: capsule_shape_row.y + 3.0,
            width: 80.0,
            height: 20.0,
        };
        self.draw_text(
            if zh { "边角" } else { "Corners" },
            capsule_shape_label_rect,
            chrome.muted_color,
        )?;
        for (index, shape) in CapsuleShapeChoice::ALL.iter().copied().enumerate() {
            let Some(option) =
                zone_editor_geometry::zone_editor_capsule_shape_option_rect(viewport, index)
            else {
                continue;
            };
            let selected = shape == selected_shape;
            let option_radius = match shape {
                CapsuleShapeChoice::Pill | CapsuleShapeChoice::Circle => {
                    BorderRadius::all(option.height * 0.5)
                }
                CapsuleShapeChoice::Rounded => chrome.row_radius,
                CapsuleShapeChoice::Minimal => BorderRadius::all(8.0),
                CapsuleShapeChoice::Square => BorderRadius::ZERO,
            };
            self.fill_rounded_rect(
                option,
                if selected {
                    with_alpha(chrome.accent_color, 0.18)
                } else {
                    chrome.input_background
                },
                option_radius,
            )?;
            if selected {
                self.stroke_rounded_rect(
                    option,
                    with_alpha(chrome.accent_color, 0.82),
                    option_radius,
                    1.0,
                )?;
            }
            let label = match (zh, shape) {
                (true, CapsuleShapeChoice::Pill) => "胶囊",
                (true, CapsuleShapeChoice::Rounded) => "圆角",
                (true, CapsuleShapeChoice::Circle) => "圆形",
                (true, CapsuleShapeChoice::Minimal) => "极简",
                (true, CapsuleShapeChoice::Square) => "方角",
                (false, CapsuleShapeChoice::Pill) => "Pill",
                (false, CapsuleShapeChoice::Rounded) => "Rounded",
                (false, CapsuleShapeChoice::Circle) => "Circle",
                (false, CapsuleShapeChoice::Minimal) => "Minimal",
                (false, CapsuleShapeChoice::Square) => "Square",
            };
            self.draw_text_no_wrap_with_style(
                label,
                option,
                if selected {
                    chrome.body_color
                } else {
                    chrome.muted_color
                },
                11.5,
                if selected { 600 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: panel.x + 1.0,
                y: panel.bottom() - 64.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let save_rect = zone_editor_geometry::zone_editor_save_rect(viewport);
        self.fill_rounded_rect(save_rect, chrome.accent_color, chrome.button_radius)?;
        self.draw_text_no_wrap_with_style(
            if zh { "保存" } else { "Save" },
            save_rect,
            tauri_palette.readable_text_on(chrome.accent_color),
            13.0,
            600,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        let cancel_rect = zone_editor_geometry::zone_editor_cancel_rect(viewport);
        self.fill_rounded_rect(cancel_rect, chrome.input_background, chrome.button_radius)?;
        self.draw_text_no_wrap_with_style(
            if zh { "取消" } else { "Cancel" },
            cancel_rect,
            chrome.body_color,
            13.0,
            500,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )?;
        Ok(())
    }

    fn draw_item_file_rename_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        // The rename form is a movable auxiliary HWND. Keep only its rounded
        // outer corners transparent; a translucent card exposes sharp desktop
        // and foreground-window seams through the text fields.
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
        )?;

        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 26.0,
        };
        // M6c — file rename panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh { "重命名文件" } else { "Rename file" },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.item_file_rename.borrow();
        let current_path = session
            .as_ref()
            .map(|entry| entry.current_path.as_str())
            .unwrap_or(if zh {
                "未选择任何项目"
            } else {
                "No item selected"
            });
        let path_rect = item_file_rename_geometry::item_file_rename_path_rect(viewport);
        self.draw_text(current_path, path_rect, chrome.muted_color)?;

        let label_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 84.0,
            width: panel.width - 36.0,
            height: 18.0,
        };
        self.draw_text(
            if zh { "新文件名" } else { "New file name" },
            label_rect,
            chrome.muted_color,
        )?;

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
            .unwrap_or((
                if zh {
                    "按 Enter 确认重命名，按 Esc 取消。"
                } else {
                    "Enter to rename; Esc to cancel."
                },
                chrome.muted_color,
            ));
        let status_rect = item_file_rename_geometry::item_file_rename_status_rect(viewport);
        self.draw_text(status.0, status_rect, status.1)?;
        Ok(())
    }

    fn draw_icon_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        let close_rect = picker_geometry::icon_picker_close_rect(viewport);
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: (close_rect.x - panel.x - 28.0).max(120.0),
            height: 28.0,
        };
        // M6c — icon picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "选择区域图标"
            } else {
                "Icon picker"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.fill_rounded_rect(
            close_rect,
            with_alpha(chrome.body_color, 0.06),
            chrome.slot_radius,
        )?;
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close_rect, 14.0),
            chrome.muted_color,
        )?;

        let session = app.icon_picker.borrow();
        let selected_icon = session
            .as_ref()
            .map(|s| s.selected_icon.as_str())
            .unwrap_or("");
        let selected_icon_label = if selected_icon.is_empty() {
            if zh { "未选择" } else { "No selection" }
        } else {
            localized_icon_wire_label(selected_icon, zh)
        };
        let target_label = match session.as_ref().and_then(|s| s.zone_id) {
            Some(_) if zh => "应用到当前区域",
            Some(_) => "Target: zone icon",
            None if session.is_some() && zh => "应用到批量管理器中的已选区域",
            None if session.is_some() => "Target: BulkManager selection",
            None if zh => "尚未选择应用目标",
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
        self.draw_text(selected_icon_label, selected_rect, chrome.body_color)?;

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
                x: slot_rect.x + (slot_rect.width - 22.0) * 0.5,
                y: slot_rect.y + 6.0,
                width: 22.0,
                height: 22.0,
            };
            self.draw_svg_document_stroke_fit(
                kind.source_svg(),
                icon_rect,
                chrome.body_color,
                22.0,
            )?;
            let slug_rect = bento_nano_style::Rect {
                x: slot_rect.x + 3.0,
                y: slot_rect.y + 32.0,
                width: slot_rect.width - 6.0,
                height: 18.0,
            };
            self.draw_text_no_wrap_with_style(
                icon_kind_label(*kind, zh),
                slug_rect,
                chrome.body_color,
                9.5,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        let hint_rect = picker_geometry::icon_picker_hint_rect(viewport, ALL_ICON_KINDS.len());
        self.draw_text(
            if zh {
                "单击图标即可保存；方向键可切换，Esc 取消。"
            } else {
                "Click an icon to save. F2 or Right cycles icon. Enter saves. Esc cancels."
            },
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
                if zh {
                    "请从区域菜单打开图标选择器。"
                } else {
                    "Open from a zone to commit the selected icon."
                },
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }

    fn draw_palette_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        let title_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: panel.y + 16.0,
            width: panel.width - 36.0,
            height: 28.0,
        };
        // M6c — palette picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "选择强调色"
            } else {
                "Palette picker"
            },
            title_rect,
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;

        let session = app.palette_picker.borrow();
        let target_label = match session.as_ref().map(|s| s.target) {
            Some(PaletteTarget::ZoneAccent(_)) if zh => "应用到当前区域",
            Some(PaletteTarget::ZoneAccent(_)) => "Target: zone accent",
            Some(PaletteTarget::ThemeBase) if zh => "应用到当前主题",
            Some(PaletteTarget::ThemeBase) => "Target: theme base accent",
            Some(PaletteTarget::BulkManagerSelectedAccent) if zh => "应用到批量管理器中的已选区域",
            Some(PaletteTarget::BulkManagerSelectedAccent) => "Target: BulkManager selection",
            None if zh => "尚未选择应用目标",
            None => "Target: none",
        };
        let selected_accent = session
            .as_ref()
            .and_then(|s| s.selected_accent.as_deref())
            .unwrap_or(if zh { "未设置" } else { "None" });

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
        self.draw_text(
            if zh { "清除" } else { "Clear" },
            clear_text_rect,
            chrome.body_color,
        )?;

        let value_rect = picker_geometry::palette_picker_value_rect(viewport);
        self.draw_text(selected_accent, value_rect, chrome.body_color)?;

        let hint_rect = picker_geometry::palette_picker_hint_rect(viewport);
        self.draw_text(
            if zh {
                "单击色块即可保存；选择“清除”可恢复默认，Esc 取消。"
            } else {
                "Click a swatch or Clear to save. F3/Right cycles. Esc cancels."
            },
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
                if zh {
                    "请先从区域或设置页面打开颜色选择器。"
                } else {
                    "No palette target is active."
                },
                warning_rect,
                chrome.warning_color,
            )?;
        }
        Ok(())
    }

    fn draw_capsule_picker_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — capsule picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "场景胶囊"
            } else {
                "Context Capsules"
            },
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
            if zh {
                "保存当前桌面布局，并在需要时一键恢复。"
            } else {
                "Save the current Desktop layout and restore it whenever you need it."
            },
            capsule_picker::capsule_picker_hint_rect(viewport),
            chrome.muted_color,
        )?;

        let state = app.capsule_picker.borrow();
        let action_palette = app.active_theme_tauri();
        for (index, hit) in capsule_picker::CAPSULE_PICKER_ACTIONS
            .iter()
            .copied()
            .enumerate()
        {
            let rect = capsule_picker::capsule_picker_action_rect(viewport, index);
            let enabled = !state.is_busy()
                && (!matches!(hit, CapsulePickerHit::Restore | CapsulePickerHit::Delete)
                    || !state.entries().is_empty());
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match hit {
                    CapsulePickerHit::Capture => AuxiliaryActionEmphasis::Primary,
                    CapsulePickerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    CapsulePickerHit::Restore
                    | CapsulePickerHit::Close
                    | CapsulePickerHit::Hint
                    | CapsulePickerHit::Error
                    | CapsulePickerHit::Empty
                    | CapsulePickerHit::Row(_) => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.row_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.row_radius, 1.0)?;
            self.draw_text_aligned(
                capsule_action_label(hit, zh),
                rect,
                action.text,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        if let Some(error) = state.last_error() {
            self.draw_text(
                error,
                capsule_picker::capsule_picker_error_rect(viewport),
                chrome.error_color,
            )?;
        }
        if state.entries().is_empty() {
            let empty = capsule_picker::capsule_picker_empty_rect(viewport);
            self.draw_icon_glyph(
                IconKind::Bookmark.as_str(),
                bento_nano_style::Rect {
                    x: empty.x + (empty.width - 32.0) * 0.5,
                    y: empty.y,
                    width: 32.0,
                    height: 32.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有场景胶囊"
                } else {
                    "No context capsules yet"
                },
                bento_nano_style::Rect {
                    x: empty.x,
                    y: empty.y + 42.0,
                    width: empty.width,
                    height: 24.0,
                },
                chrome.body_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.draw_text_aligned(
                if zh {
                    "选择“保存当前”即可记录这组桌面布局。"
                } else {
                    "Select Save current to capture this Desktop layout."
                },
                bento_nano_style::Rect {
                    x: empty.x,
                    y: empty.y + 72.0,
                    width: empty.width,
                    height: 24.0,
                },
                chrome.muted_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
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
            let icon = IconKind::from_str_opt(entry.icon.as_str()).unwrap_or(IconKind::Bookmark);
            self.draw_icon_glyph(
                icon.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 10.0,
                    y: row.y + 10.0,
                    width: 20.0,
                    height: 20.0,
                },
                chrome.body_color,
            )?;
            self.draw_text_no_wrap(
                entry.name.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 42.0,
                    y: row.y + 5.0,
                    width: row.width - 52.0,
                    height: 18.0,
                },
                chrome.body_color,
            )?;
            self.draw_text_no_wrap(
                entry.captured_at.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 42.0,
                    y: row.y + 22.0,
                    width: row.width - 52.0,
                    height: 16.0,
                },
                chrome.muted_color,
            )?;
        }
        Ok(())
    }

    fn draw_bulk_manager_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the BulkManager panel.
        use bento_nano_style::tokens as style_tokens;
        let action_palette = app.active_theme_tauri();
        let chrome = bulk_manager_panel::BulkManagerChrome::from_tauri_tokens(
            action_palette,
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = bulk_manager_panel::bulk_manager_panel_rect(viewport);
        let search_rect = bulk_manager_panel::bulk_manager_search_rect(viewport);
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — bulk manager panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "批量管理区域"
            } else {
                "Bulk Manager"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: (search_rect.x - panel.x - 30.0).max(160.0),
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close_rect = bulk_manager_panel::bulk_manager_close_rect(viewport);
        let close_chrome =
            auxiliary_action_chrome(action_palette, AuxiliaryActionEmphasis::Secondary);
        self.fill_rounded_rect(close_rect, close_chrome.fill, chrome.button_radius)?;
        self.stroke_rounded_rect(close_rect, close_chrome.border, chrome.button_radius, 1.0)?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            close_chrome.text,
        )?;

        let bulk_line_height =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            if zh {
                "搜索与排序区域；单击一行即可加入或移出批量选择。"
            } else {
                "Search and sort zones; click a row to toggle its batch selection."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + bulk_manager_panel::RUNTIME_HELPER_TOP_PX,
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
        if state.search_focused() {
            self.stroke_rounded_rect(
                search_rect,
                with_alpha(action_palette.accent_blue, 0.88),
                chrome.search_radius,
                1.5,
            )?;
        }
        let search_body = if state.search().is_empty() {
            if zh {
                "搜索区域…"
            } else {
                "Search zones..."
            }
        } else {
            state.search()
        };
        let search_text = if zh {
            smol_str::SmolStr::new(search_body)
        } else {
            smol_str::SmolStr::new(format!("Search: {search_body}"))
        };
        self.draw_text(
            search_text.as_str(),
            bento_nano_style::Rect {
                x: search_rect.x + 10.0,
                y: search_rect.y + 7.0,
                width: search_rect.width - 20.0,
                height: 18.0,
            },
            if state.search().is_empty() {
                chrome.muted_color
            } else {
                chrome.body_color
            },
        )?;
        if state.search_focused() && state.search().is_empty() {
            self.fill_rounded_rect(
                bento_nano_style::Rect {
                    x: search_rect.x + 10.0,
                    y: search_rect.y + 9.0,
                    width: 1.5,
                    height: search_rect.height - 18.0,
                },
                action_palette.accent_blue,
                BorderRadius::ZERO,
            )?;
        }
        let rows = state.visible_rows();
        let row_window_start =
            bulk_manager_panel::bulk_manager_visible_window_start(state.cursor_index(), rows.len());
        let row_window_summary = localized_visible_range(
            row_window_start,
            rows.len(),
            bulk_manager_panel::RUNTIME_VISIBLE_ROW_LIMIT,
            zh,
        );
        let selected_count = state.selected().len();
        let base_status_text = app.bulk_manager_status.borrow().clone().unwrap_or_else(|| {
            if zh {
                smol_str::SmolStr::new(format!(
                    "共 {} 个区域，已选择 {} 个",
                    rows.len(),
                    selected_count
                ))
            } else {
                smol_str::SmolStr::new(format!(
                    "{} zones listed, {} selected",
                    rows.len(),
                    selected_count
                ))
            }
        });
        let base_status_text = if let Some(summary) = row_window_summary {
            smol_str::SmolStr::new(format!("{base_status_text} — {summary}"))
        } else {
            base_status_text
        };
        let edit_status = state.text_edit().map(|edit| {
            let draft = if edit.draft.is_empty() {
                bulk_text_edit_placeholder(edit.field, zh)
            } else {
                edit.draft.as_str()
            };
            if zh {
                smol_str::SmolStr::new(format!(
                    "编辑{}：{}　F2 切换字段 · Enter 应用 · Esc 取消",
                    bulk_text_edit_field_label(edit.field, true),
                    draft
                ))
            } else {
                smol_str::SmolStr::new(format!(
                    "Edit {}: {}    F2 field · Enter apply · Esc cancel",
                    bulk_text_edit_field_label(edit.field, false),
                    draft
                ))
            }
        });
        let status_text = edit_status.as_ref().unwrap_or(&base_status_text);
        let status_top = panel.y + bulk_manager_panel::RUNTIME_STATUS_TOP_PX;
        let status_rect = bento_nano_style::Rect {
            x: panel.x + 18.0,
            y: status_top,
            width: panel.width - 36.0,
            height: bulk_line_height,
        };
        if edit_status.is_some() {
            self.fill_rounded_rect(status_rect, chrome.cursor_background, chrome.edit_radius)?;
        }
        self.draw_text(
            status_text.as_str(),
            if edit_status.is_some() {
                inset_rect(status_rect, 4.0)
            } else {
                status_rect
            },
            if edit_status.is_some() {
                chrome.body_color
            } else {
                chrome.muted_color
            },
        )?;
        // Quiet separators make the dense toolbar read as three command
        // groups (selection, visibility, layout) instead of fifteen unrelated
        // pill buttons.
        for (x, y) in [
            (panel.x + 133.0, panel.y + 108.0),
            (panel.x + 249.0, panel.y + 108.0),
            (panel.x + 554.0, panel.y + 138.0),
        ] {
            self.fill_rounded_rect(
                bento_nano_style::Rect {
                    x,
                    y,
                    width: 1.0,
                    height: 16.0,
                },
                with_alpha(chrome.body_color, 0.12),
                BorderRadius::ZERO,
            )?;
        }
        for spec in bulk_manager_panel::BULK_MANAGER_ACTION_BUTTONS {
            let rect = bulk_manager_panel::bulk_manager_button_rect(viewport, *spec);
            let enabled = bulk_manager_panel::bulk_manager_action_enabled(
                spec.hit,
                !rows.is_empty(),
                selected_count > 0,
            );
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    bulk_manager_panel::BulkManagerPointerHit::Delete => {
                        AuxiliaryActionEmphasis::Danger
                    }
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            if enabled {
                self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
                self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            }
            // RC-4 Gap 3 — `draw_text_no_wrap` keeps the 4-letter button
            // labels ("Show", "Move", "Close") on a single line and trims
            // with an ellipsis if the layout box is too narrow, instead of
            // wrapping them into "Sho/w", "Mov", "Clos/e" against the wide
            // YaHei UI fallback Latin metrics. Shrink the horizontal pad
            // from 7 px to SPACING.xs (4 px) each side to give the run an
            // extra 6 px of room — enough for every label in the table to
            // measure clean at the spec'd width without column changes.
            self.draw_text_no_wrap_with_style(
                bulk_manager_action_label(spec.hit, zh),
                rect,
                if enabled {
                    action.text
                } else {
                    with_alpha(chrome.muted_color, 0.42)
                },
                11.5,
                if enabled { 550 } else { 450 },
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        let sort_band = bento_nano_style::Rect {
            x: panel.x + bulk_manager_panel::RUNTIME_PANEL_INSET_PX,
            y: panel.y + bulk_manager_panel::RUNTIME_SORT_HEADER_TOP_PX,
            width: panel.width - bulk_manager_panel::RUNTIME_PANEL_INSET_PX * 2.0,
            height: bulk_manager_panel::RUNTIME_SORT_HEADER_HEIGHT_PX,
        };
        self.fill_rounded_rect(sort_band, chrome.row_background, chrome.sort_radius)?;
        for key in bulk_manager_panel::SortKey::ALL {
            let rect = bulk_manager_panel::bulk_manager_sort_header_rect(viewport, *key);
            let active = state.sort_key() == *key;
            let suffix = if active {
                match state.sort_direction() {
                    bulk_manager_panel::SortDirection::Ascending => " ↑",
                    bulk_manager_panel::SortDirection::Descending => " ↓",
                }
            } else {
                ""
            };
            let label =
                smol_str::SmolStr::new(format!("{}{}", bulk_manager_sort_label(*key, zh), suffix));
            // RC-4 Gap 3 — same no-wrap protection as the action buttons.
            self.draw_text_no_wrap_with_style(
                label.as_str(),
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    width: (rect.width - 16.0).max(0.0),
                    ..rect
                },
                if active {
                    action_palette.accent_blue
                } else {
                    chrome.muted_color
                },
                11.5,
                if active { 600 } else { 500 },
                1.0,
                dwrite::TextAlign {
                    h: if matches!(
                        key,
                        bulk_manager_panel::SortKey::Items | bulk_manager_panel::SortKey::Size
                    ) {
                        dwrite::HAlign::Center
                    } else {
                        dwrite::HAlign::Leading
                    },
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        if rows.is_empty() {
            self.draw_text(
                if zh {
                    "暂无可批量管理的区域。"
                } else {
                    "No zones available for bulk operations."
                },
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
            let selected = state.is_selected(row_data.id);
            let name_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Name,
            );
            let checkbox = bento_nano_style::Rect {
                x: name_cell.x + 9.0,
                y: name_cell.y + (name_cell.height - 14.0) * 0.5,
                width: 14.0,
                height: 14.0,
            };
            if selected {
                self.fill_rounded_rect(
                    checkbox,
                    action_palette.accent_blue,
                    BorderRadius::all(4.0),
                )?;
                self.draw_text_no_wrap_with_style(
                    "✓",
                    checkbox,
                    action_palette.readable_text_on(action_palette.accent_blue),
                    10.0,
                    700,
                    1.0,
                    dwrite::TextAlign {
                        h: dwrite::HAlign::Center,
                        v: dwrite::VAlign::Center,
                    },
                )?;
            } else {
                self.stroke_rounded_rect(
                    checkbox,
                    if index == state.cursor_index() {
                        action_palette.accent_blue
                    } else {
                        with_alpha(chrome.muted_color, 0.5)
                    },
                    BorderRadius::all(4.0),
                    1.0,
                )?;
            }
            let name_x = checkbox.right() + 9.0;
            self.draw_text_no_wrap_with_style(
                row_data.display_name.as_str(),
                bento_nano_style::Rect {
                    x: name_x,
                    y: name_cell.y + 3.0,
                    width: (name_cell.right() - name_x - 8.0).max(0.0),
                    height: 18.0,
                },
                chrome.body_color,
                12.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let row_meta = smol_str::SmolStr::new(if zh {
                format!(
                    "{} · {}",
                    if row_data.visible { "显示" } else { "隐藏" },
                    if row_data.locked {
                        "已锁定"
                    } else {
                        "未锁定"
                    }
                )
            } else {
                format!(
                    "{} · {}",
                    if row_data.visible {
                        "Visible"
                    } else {
                        "Hidden"
                    },
                    if row_data.locked {
                        "Locked"
                    } else {
                        "Unlocked"
                    }
                )
            });
            self.draw_text_no_wrap_with_style(
                row_meta.as_str(),
                bento_nano_style::Rect {
                    x: name_x,
                    y: name_cell.y + 20.0,
                    width: (name_cell.right() - name_x - 8.0).max(0.0),
                    height: 14.0,
                },
                chrome.muted_color,
                9.5,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let items_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Items,
            );
            let item_count = smol_str::SmolStr::new(row_data.item_count.to_string());
            self.draw_text_no_wrap_with_style(
                item_count.as_str(),
                items_cell,
                chrome.body_color,
                12.0,
                500,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let accent_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Accent,
            );
            let parsed_accent = parse_hex_color(row_data.accent_hex.as_str());
            let accent_text = if row_data.accent_hex.is_empty() {
                if zh { "默认" } else { "Default" }
            } else {
                row_data.accent_hex.as_str()
            };
            let accent_text_x = if let Some(color) = parsed_accent {
                let swatch = bento_nano_style::Rect {
                    x: accent_cell.x + 8.0,
                    y: accent_cell.y + (accent_cell.height - 12.0) * 0.5,
                    width: 12.0,
                    height: 12.0,
                };
                self.fill_rounded_rect(swatch, color, BorderRadius::all(6.0))?;
                swatch.right() + 6.0
            } else {
                accent_cell.x + 8.0
            };
            self.draw_text_no_wrap(
                accent_text,
                bento_nano_style::Rect {
                    x: accent_text_x,
                    y: accent_cell.y + 7.0,
                    width: (accent_cell.right() - accent_text_x - 6.0).max(0.0),
                    height: 18.0,
                },
                chrome.body_color,
            )?;

            let size_cell = bulk_manager_panel::bulk_manager_row_cell_rect(
                viewport,
                display_index,
                bulk_manager_panel::SortKey::Size,
            );
            let size_text = smol_str::SmolStr::new(format!(
                "{}×{}%",
                row_data.width_percent, row_data.height_percent
            ));
            self.draw_text_no_wrap_with_style(
                size_text.as_str(),
                size_cell,
                chrome.body_color,
                12.0,
                500,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }
        Ok(())
    }

    fn draw_timeline_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — timeline panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "桌面时间线"
            } else {
                "Desktop Timeline"
            },
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
            if zh {
                "选择记录可预览布局；使用上方按钮保存、固定、恢复或删除。"
            } else {
                "Select a checkpoint to preview it, then save, pin, restore, or delete."
            },
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
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 条时间线记录", state.entries().len())
            } else {
                format!("Loaded {} checkpoints", state.entries().len())
            })
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

        let has_entries = !state.entries().is_empty();
        let action_palette = app.active_theme_tauri();
        for spec in timeline_panel::TIMELINE_ACTION_BUTTONS {
            let rect = timeline_panel::timeline_button_rect(viewport, *spec);
            let enabled = !matches!(
                spec.hit,
                timeline_panel::TimelinePointerHit::Pin
                    | timeline_panel::TimelinePointerHit::Restore
                    | timeline_panel::TimelinePointerHit::Delete
            ) || has_entries;
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    timeline_panel::TimelinePointerHit::Save => AuxiliaryActionEmphasis::Primary,
                    timeline_panel::TimelinePointerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap(
                timeline_action_label(spec.hit, zh),
                bento_nano_style::Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 6.0,
                    width: rect.width - 16.0,
                    height: 16.0,
                },
                action.text,
            )?;
        }

        if !has_entries {
            let center_y = panel.y + panel.height * 0.56;
            self.draw_icon_glyph(
                IconKind::Camera.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + (panel.width - 34.0) * 0.5,
                    y: center_y - 48.0,
                    width: 34.0,
                    height: 34.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有时间线记录"
                } else {
                    "No timeline checkpoints yet"
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.draw_text_aligned(
                if zh {
                    "选择“保存”记录当前区域布局，之后可随时预览和恢复。"
                } else {
                    "Select Save to capture the current layout for preview and restore."
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y + 30.0,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.muted_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
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
            let line = smol_str::SmolStr::new(if zh {
                format!(
                    "{}　· {} 个区域　· {} 个项目",
                    entry.captured_at, entry.zone_count, entry.item_count
                )
            } else {
                format!(
                    "{}  · {} zones  · {} items",
                    entry.captured_at, entry.zone_count, entry.item_count
                )
            });
            if entry.pinned {
                self.draw_icon_glyph(
                    IconKind::Pin.as_str(),
                    bento_nano_style::Rect {
                        x: row.x + 10.0,
                        y: row.y + 5.0,
                        width: 12.0,
                        height: 12.0,
                    },
                    chrome.body_color,
                )?;
            }
            self.draw_text(
                line.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 28.0,
                    y: row.y + 4.0,
                    width: row.width - 38.0,
                    height: 17.0,
                },
                chrome.body_color,
            )?;
            let delta = if entry.delta_summary.is_empty() {
                if zh { "无变化" } else { "no change" }
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
            let detail = smol_str::SmolStr::new(if zh {
                format!(
                    "当前记录\n{} · {} 个区域 · {}",
                    if active.pinned {
                        "已固定"
                    } else {
                        "未固定"
                    },
                    active.snapshot.zones.len(),
                    active.snapshot.captured_at
                )
            } else {
                format!(
                    "Selected checkpoint\n{} · {} zones · {}",
                    if active.pinned {
                        "Pinned"
                    } else {
                        "Not pinned"
                    },
                    active.snapshot.zones.len(),
                    active.snapshot.captured_at
                )
            });
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
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        // Wave E: Tauri SSoT tokens for the Snapshot picker panel.
        use bento_nano_style::tokens as style_tokens;
        let chrome = snapshot_picker::SnapshotPickerChrome::from_tauri_tokens(
            app.active_theme_tauri(),
            app.active_theme_radius_tauri(),
            app.active_theme_shadow_tauri(),
        );
        let viewport = app.viewport;
        let panel = snapshot_picker::snapshot_picker_panel_rect(viewport);
        let action_palette = app.active_theme_tauri();
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            with_alpha(chrome.body_color, 0.12),
            chrome.panel_radius,
            1.0,
        )?;
        let close_rect = snapshot_picker::snapshot_picker_close_rect(viewport);
        let close_chrome =
            auxiliary_action_chrome(action_palette, AuxiliaryActionEmphasis::Secondary);
        self.fill_rounded_rect(close_rect, close_chrome.fill, chrome.button_radius)?;
        self.draw_icon_glyph(
            "x",
            centered_square_rect(close_rect, 14.0),
            close_chrome.text,
        )?;
        // M6c — snapshot picker panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "布局快照"
            } else {
                "Layout Snapshots"
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 14.0,
                width: (close_rect.x - panel.x - 30.0).max(120.0),
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: panel.x + 1.0,
                y: panel.y + 51.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let helper_line_h =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        self.draw_text(
            if zh {
                "选择快照查看预览，再载入或删除；也可保存当前布局。"
            } else {
                "Select a snapshot to preview, load, or delete; save the current layout anytime."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 60.0,
                width: panel.width - 36.0,
                height: helper_line_h,
            },
            chrome.muted_color,
        )?;

        let state = app.snapshot_picker.borrow();
        let status = if let Some(error) = state.error() {
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = state.status() {
            status.clone()
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 个布局快照", state.entries().len())
            } else {
                format!("Loaded {} snapshots", state.entries().len())
            })
        };
        let status_y = panel.y + 82.0;
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

        let has_entries = !state.entries().is_empty();
        for spec in snapshot_picker::SNAPSHOT_PICKER_ACTION_BUTTONS {
            let rect = snapshot_picker::snapshot_picker_button_rect(viewport, *spec);
            let enabled = !matches!(
                spec.hit,
                snapshot_picker::SnapshotPickerPointerHit::Load
                    | snapshot_picker::SnapshotPickerPointerHit::Delete
            ) || has_entries;
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    snapshot_picker::SnapshotPickerPointerHit::Save => {
                        AuxiliaryActionEmphasis::Primary
                    }
                    snapshot_picker::SnapshotPickerPointerHit::Delete => {
                        AuxiliaryActionEmphasis::Danger
                    }
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap_with_style(
                snapshot_action_label(spec.hit, zh),
                rect,
                action.text,
                12.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
        }

        if !has_entries {
            let center_y = panel.y + panel.height * 0.56;
            self.draw_icon_glyph(
                IconKind::Camera.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + (panel.width - 32.0) * 0.5,
                    y: center_y - 46.0,
                    width: 32.0,
                    height: 32.0,
                },
                chrome.muted_color,
            )?;
            self.draw_text_aligned(
                if zh {
                    "还没有布局快照"
                } else {
                    "No layout snapshots yet"
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.body_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.draw_text_aligned(
                if zh {
                    "选择“保存”即可创建第一份快照。"
                } else {
                    "Select Save to create your first snapshot."
                },
                bento_nano_style::Rect {
                    x: panel.x + 18.0,
                    y: center_y + 30.0,
                    width: panel.width - 36.0,
                    height: 24.0,
                },
                chrome.muted_color,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
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
            let meta = snapshot_picker::meta_line(
                snapshot,
                snapshot.captured_at.as_str(),
                if zh { "区域" } else { "Zones" },
            );
            let confirm = state.row_action().is_awaiting_for(snapshot.id.as_str());
            let meta_text = if confirm {
                smol_str::SmolStr::new(if zh {
                    format!("{meta}　·　再次选择删除以确认")
                } else {
                    format!("{meta}  ·  Select Delete again to confirm")
                })
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
            self.draw_text(
                if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                    "暂无区域"
                } else {
                    "No zones"
                },
                inset_rect(rect, 8.0),
                chrome.empty_text_color,
            )?;
        }
        Ok(())
    }

    fn draw_rules_wizard_window(&mut self, app: &AppState) -> Result<(), RenderError> {
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
        let chrome = rules_wizard::RulesWizardChrome::from_tokens(
            app.active_theme_palette(),
            app.active_theme_radius(),
            app.active_theme_shadow(),
        );
        let viewport = app.viewport;
        let panel = rules_wizard::rules_wizard_panel_rect(viewport);
        let shadow_rect = rules_wizard::rules_wizard_panel_shadow_rect(panel, chrome.panel_shadow);
        self.fill_rounded_rect(shadow_rect, chrome.panel_shadow.color, chrome.panel_radius)?;
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — rules wizard panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "自动整理规则"
            } else {
                "Automation Rules"
            },
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
            if zh {
                "按步骤设置条件与操作；完成后可预览、保存或运行规则。"
            } else {
                "Configure conditions and actions step by step, then preview, save, or run."
            },
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
        let rule_window_summary = localized_visible_range(
            rule_window_start,
            rules.len(),
            rules_wizard::RUNTIME_VISIBLE_RULE_LIMIT,
            zh,
        );
        let status = app.rules_wizard_status.borrow().clone();
        let step = wizard.step();
        let step_line = smol_str::SmolStr::new(if zh {
            format!(
                "步骤 {}/{}　· {}　· {}　· {}",
                step.index(),
                WizardStep::TOTAL,
                wizard_step_label(step, true),
                if wizard.is_complete() {
                    "已完成"
                } else {
                    "编辑中"
                },
                if wizard.enabled() {
                    "已启用"
                } else {
                    "已停用"
                }
            )
        } else {
            format!(
                "Step {}/{} · {} · {} · {}",
                step.index(),
                WizardStep::TOTAL,
                wizard_step_label(step, false),
                if wizard.is_complete() {
                    "Complete"
                } else {
                    "Editing"
                },
                if wizard.enabled() {
                    "Enabled"
                } else {
                    "Disabled"
                }
            )
        });
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
            smol_str::SmolStr::new(if zh {
                format!("错误：{error}")
            } else {
                format!("Error: {error}")
            })
        } else if let Some(status) = status {
            status
        } else {
            smol_str::SmolStr::new(if zh {
                format!("已载入 {} 条规则", rules.len())
            } else {
                format!("Loaded {} saved rules", rules.len())
            })
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

        let has_saved_rules = !rules.is_empty();
        let has_conditions = !wizard.conditions().is_empty();
        let action_palette = app.active_theme_tauri();
        for spec in rules_wizard::RULES_WIZARD_ACTION_BUTTONS {
            let rect = rules_wizard::rules_wizard_button_rect(viewport, *spec);
            let enabled = match spec.hit {
                rules_wizard::RulesWizardPointerHit::Edit
                | rules_wizard::RulesWizardPointerHit::Run
                | rules_wizard::RulesWizardPointerHit::Delete => has_saved_rules,
                rules_wizard::RulesWizardPointerHit::RemoveCondition
                | rules_wizard::RulesWizardPointerHit::NextCondition => has_conditions,
                _ => true,
            };
            let emphasis = if !enabled {
                AuxiliaryActionEmphasis::Disabled
            } else {
                match spec.hit {
                    rules_wizard::RulesWizardPointerHit::NextSave => {
                        AuxiliaryActionEmphasis::Primary
                    }
                    rules_wizard::RulesWizardPointerHit::Delete => AuxiliaryActionEmphasis::Danger,
                    _ => AuxiliaryActionEmphasis::Secondary,
                }
            };
            let action = auxiliary_action_chrome(action_palette, emphasis);
            self.fill_rounded_rect(rect, action.fill, chrome.button_radius)?;
            self.stroke_rounded_rect(rect, action.border, chrome.button_radius, 1.0)?;
            self.draw_text_no_wrap(
                rules_action_label(spec.hit, step, zh),
                bento_nano_style::Rect {
                    x: rect.x + 6.0,
                    y: rect.y + 5.0,
                    width: rect.width - 12.0,
                    height: 16.0,
                },
                action.text,
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
        let condition_window_summary = localized_visible_range(
            condition_window_start,
            condition_count,
            rules_wizard::RUNTIME_VISIBLE_CONDITION_LIMIT,
            zh,
        );
        let action = wizard.action();
        let action_text = smol_str::SmolStr::new(if zh {
            format!(
                "执行：{}　· {}",
                action_label(action.kind, true),
                if action.value.trim().is_empty() {
                    "请填写目标"
                } else {
                    action.value.as_str()
                }
            )
        } else {
            format!(
                "Action: {} · {}",
                action_label(action.kind, false),
                if action.value.trim().is_empty() {
                    "Enter a target"
                } else {
                    action.value.as_str()
                }
            )
        });
        let name_text = smol_str::SmolStr::new(if zh {
            format!(
                "名称：{}",
                if wizard.name().trim().is_empty() {
                    "请填写规则名称"
                } else {
                    wizard.name()
                }
            )
        } else {
            format!(
                "Name: {}",
                if wizard.name().trim().is_empty() {
                    "Enter a rule name"
                } else {
                    wizard.name()
                }
            )
        });
        let run_text = smol_str::SmolStr::new(if zh {
            format!(
                "运行方式：{}　· 每 {} 分钟",
                run_mode_label(wizard.run_mode(), true),
                wizard.interval_minutes()
            )
        } else {
            format!(
                "Run: {} · every {} min",
                run_mode_label(wizard.run_mode(), false),
                wizard.interval_minutes()
            )
        });
        let preview_text = smol_str::SmolStr::new(if zh {
            if wizard.preview_busy() {
                "预览：正在计算…".to_owned()
            } else {
                format!("预览：命中 {} 项", wizard.preview_hits().len())
            }
        } else if wizard.preview_busy() {
            "Preview: calculating…".to_owned()
        } else {
            format!("Preview: {} matches", wizard.preview_hits().len())
        });

        let conditions_heading = if let Some(summary) = condition_window_summary {
            smol_str::SmolStr::new(format!(
                "{} [{}] — {summary}",
                if zh { "条件" } else { "Conditions" },
                combine_label(wizard.combine(), zh)
            ))
        } else {
            smol_str::SmolStr::new(format!(
                "{} [{}]",
                if zh { "条件" } else { "Conditions" },
                combine_label(wizard.combine(), zh)
            ))
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
                if zh {
                    "尚未添加条件"
                } else {
                    "No conditions"
                },
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
                    "{} {}. {} · {}",
                    if selected { "›" } else { " " },
                    row_index + 1,
                    predicate_label(row.kind, zh),
                    if row.value.trim().is_empty() {
                        if zh {
                            "请填写条件值"
                        } else {
                            "Enter a value"
                        }
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
            if zh { "已保存规则" } else { "Saved rules" },
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
                if zh {
                    "暂无已保存规则。完成左侧步骤后选择“下一步/保存”。"
                } else {
                    "No rules saved yet. Complete the steps and select Next/Save."
                },
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
                let text = smol_str::SmolStr::new(if zh {
                    format!(
                        "{} {}　· {}",
                        if selected { "›" } else { " " },
                        rule.name,
                        if rule.enabled {
                            "已启用"
                        } else {
                            "已停用"
                        }
                    )
                } else {
                    format!(
                        "{} {} · {}",
                        if selected { "›" } else { " " },
                        rule.name,
                        if rule.enabled { "Enabled" } else { "Disabled" }
                    )
                });
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
            let line = rules_preview_hit_label(hit, index, zh);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        self.stroke_rounded_rect(
            panel,
            app.active_theme_tauri().border_expanded,
            chrome.panel_radius,
            1.0,
        )?;
        use bento_nano_style::i18n_zh_cn::ids;
        // M6c — search panel title (`h2`).
        self.draw_text_chromatic_title(
            bento_nano_style::t(ids::SEARCH_TITLE),
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: panel.y + 18.0,
                width: panel.width - 76.0,
                height: 28.0,
            },
            chrome.title_color,
            app.active_theme_effect_tauri(),
        )?;
        let close = search_bar::search_close_rect(viewport);
        self.draw_icon_glyph(
            IconKind::X.as_str(),
            centered_square_rect(close, 16.0),
            chrome.muted_color,
        )?;

        let state = app.search_bar.borrow();
        let input = search_bar::search_input_rect(viewport);
        self.fill_rounded_rect(input, chrome.input_background, chrome.input_radius)?;
        self.stroke_rounded_rect(
            input,
            with_alpha(app.active_theme_tauri().accent_blue, 0.70),
            chrome.input_radius,
            1.0,
        )?;
        self.draw_icon_glyph(
            IconKind::Search.as_str(),
            bento_nano_style::Rect {
                x: input.x + 14.0,
                y: input.y + 15.0,
                width: 18.0,
                height: 18.0,
            },
            chrome.muted_color,
        )?;
        let query_text = if state.query.is_empty() {
            bento_nano_style::t(ids::SEARCH_PLACEHOLDER)
        } else {
            state.query.as_str()
        };
        self.draw_text(
            query_text,
            bento_nano_style::Rect {
                x: input.x + 42.0,
                y: input.y + 12.0,
                width: input.width - 56.0,
                height: 24.0,
            },
            if state.query.is_empty() {
                chrome.muted_color
            } else {
                chrome.body_color
            },
        )?;

        let status = if state.query.is_empty() {
            smol_str::SmolStr::new_static(bento_nano_style::t(ids::SEARCH_IDLE_HINT))
        } else if state.results.is_empty() {
            smol_str::SmolStr::new_static(bento_nano_style::t(ids::SEARCH_EMPTY))
        } else {
            smol_str::SmolStr::new(format!(
                "{}{}",
                state.visible_count(),
                bento_nano_style::t(ids::SEARCH_RESULTS_SUFFIX)
            ))
        };
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
            self.draw_icon_glyph(
                IconKind::Search.as_str(),
                bento_nano_style::Rect {
                    x: panel.x + (panel.width - 28.0) * 0.5,
                    y: input.bottom() + 70.0,
                    width: 28.0,
                    height: 28.0,
                },
                with_alpha(chrome.muted_color, 0.75),
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
            let icon_rect = bento_nano_style::Rect {
                x: row.x + 12.0,
                y: row.y + 9.0,
                width: 28.0,
                height: 28.0,
            };
            self.draw_icon_glyph(hit.icon.as_str(), icon_rect, chrome.body_color)?;
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
                bento_nano_backend::search::SearchItemKind::File => {
                    bento_nano_style::t(ids::SEARCH_KIND_FILE)
                }
                bento_nano_backend::search::SearchItemKind::Folder => {
                    bento_nano_style::t(ids::SEARCH_KIND_FOLDER)
                }
                bento_nano_backend::search::SearchItemKind::Zone => {
                    bento_nano_style::t(ids::SEARCH_KIND_ZONE)
                }
                bento_nano_backend::search::SearchItemKind::Setting => {
                    bento_nano_style::t(ids::SEARCH_KIND_SETTING)
                }
                bento_nano_backend::search::SearchItemKind::Action => {
                    bento_nano_style::t(ids::SEARCH_KIND_ACTION)
                }
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
        let zh = bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN);
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
        self.fill_rounded_rect(
            panel,
            opaque_auxiliary_surface(chrome.panel_background),
            chrome.panel_radius,
        )?;
        // M6c — smart-group suggestor panel title (`h2`).
        self.draw_text_chromatic_title(
            if zh {
                "智能分组建议"
            } else {
                "Smart grouping"
            },
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
        self.fill_rounded_rect(close, chrome.close_background, chrome.close_radius)?;
        self.draw_icon_glyph("x", centered_square_rect(close, 14.0), chrome.muted_color)?;
        self.fill_rounded_rect(
            bento_nano_style::Rect {
                x: panel.x + 1.0,
                y: panel.y + 51.0,
                width: (panel.width - 2.0).max(0.0),
                height: 1.0,
            },
            with_alpha(chrome.body_color, 0.08),
            BorderRadius::ZERO,
        )?;
        let line_height =
            style_tokens::TYPOGRAPHY.sm.size_px * style_tokens::TYPOGRAPHY.sm.line_height;
        let helper_top = panel.y + 58.0;
        self.draw_text(
            if zh {
                "选择建议查看匹配文件，按需调整范围后应用。"
            } else {
                "Select a suggestion, review its files, then refine and apply."
            },
            bento_nano_style::Rect {
                x: panel.x + 18.0,
                y: helper_top,
                width: panel.width - 36.0,
                height: line_height,
            },
            chrome.muted_color,
        )?;

        let state = app.suggestor.borrow();
        let status = app.suggestor_status.borrow().clone().unwrap_or_else(|| {
            smol_str::SmolStr::new(if zh {
                format!("已生成 {} 条分组建议", state.entries().len())
            } else {
                format!("Loaded {} suggestions", state.entries().len())
            })
        });
        let status_top = panel.y + smart_group_suggestor::RUNTIME_STATUS_TOP_PX;
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
                if zh {
                    "当前桌面扫描暂未生成可用的分组建议。"
                } else {
                    "The current Desktop scan did not produce any grouping suggestions."
                },
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
            self.stroke_rounded_rect(
                row,
                with_alpha(chrome.body_color, 0.06),
                chrome.row_radius,
                1.0,
            )?;
            let icon_rect = bento_nano_style::Rect {
                x: row.x + 12.0,
                y: row.y + ((row.height - smart_group_suggestor::ROW_ICON_SIZE_PX) * 0.5),
                width: smart_group_suggestor::ROW_ICON_SIZE_PX,
                height: smart_group_suggestor::ROW_ICON_SIZE_PX,
            };
            self.draw_icon_glyph(entry.suggestion.icon.as_str(), icon_rect, chrome.body_color)?;
            let apply = smart_group_suggestor::suggestor_apply_rect(viewport, index);
            let dismiss = smart_group_suggestor::suggestor_dismiss_rect(viewport, index);
            let badge = bento_nano_style::Rect {
                x: apply.x - 82.0,
                y: row.y + 17.0,
                width: 72.0,
                height: 20.0,
            };
            // Wave F carry-over #2: title must respect badge's left edge.
            // Drop the .max(96.0) floor so we never paint into the badge;
            // route through no-wrap so an over-wide title is character-trimmed
            // inside its box instead of stamping a fragment across the badge.
            let text_width = (badge.x - (row.x + 50.0) - 12.0).max(0.0);
            self.draw_text_no_wrap_with_style(
                localized_suggestor_group_name(entry.suggestion.name.as_str(), zh),
                bento_nano_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 6.0,
                    width: text_width,
                    height: 19.0,
                },
                chrome.body_color,
                12.5,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let summary = localized_suggestor_rule_summary(&entry.suggestion, zh);
            let meta = smol_str::SmolStr::new(if zh {
                format!(
                    "已选择 {}/{}　· {}",
                    entry.selected_path_count(),
                    entry.total_path_count(),
                    summary
                )
            } else {
                format!(
                    "{}/{} selected · {}",
                    entry.selected_path_count(),
                    entry.total_path_count(),
                    summary
                )
            });
            self.draw_text_no_wrap_with_style(
                meta.as_str(),
                bento_nano_style::Rect {
                    x: row.x + 50.0,
                    y: row.y + 29.0,
                    width: text_width,
                    height: 17.0,
                },
                chrome.muted_color,
                10.0,
                450,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Leading,
                    v: dwrite::VAlign::Center,
                },
            )?;

            let tone = smart_group_suggestor::confidence_tone(entry.suggestion.confidence);
            let (badge_bg, badge_text) =
                smart_group_suggestor::tone_colors_from_tauri_palette(tone, palette);
            self.fill_rounded_rect(badge, badge_bg, chrome.badge_radius)?;
            let confidence = smol_str::SmolStr::new(format!(
                "{} {}%",
                confidence_tone_label(tone, zh),
                (entry.suggestion.confidence * 100.0).round() as i32
            ));
            self.draw_text_no_wrap_with_style(
                confidence.as_str(),
                badge,
                badge_text,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;

            self.fill_rounded_rect(apply, chrome.action_background, chrome.action_radius)?;
            let apply_text = if state.applying_id() == Some(&entry.id) {
                if zh { "应用中" } else { "Applying" }
            } else {
                if zh { "应用" } else { "Apply" }
            };
            self.draw_text_no_wrap_with_style(
                apply_text,
                apply,
                palette.readable_text_on(chrome.action_background),
                11.0,
                600,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            self.fill_rounded_rect(
                dismiss,
                with_alpha(chrome.danger_background, 0.12),
                chrome.action_radius,
            )?;
            self.stroke_rounded_rect(
                dismiss,
                with_alpha(chrome.danger_background, 0.28),
                chrome.action_radius,
                1.0,
            )?;
            self.draw_icon_glyph(
                "x",
                centered_square_rect(dismiss, 12.0),
                chrome.danger_background,
            )?;
        }

        if let Some(entry) = state.selected_entry() {
            let preview = smart_group_suggestor::suggestor_preview_rect(viewport);
            self.fill_rounded_rect(preview, chrome.preview_background, chrome.preview_radius)?;
            self.stroke_rounded_rect(
                preview,
                with_alpha(chrome.body_color, 0.08),
                chrome.preview_radius,
                1.0,
            )?;
            let title = smol_str::SmolStr::new(format!(
                "{}：{}/{} {}",
                if zh {
                    "本次整理范围"
                } else {
                    "Files to organize"
                },
                entry.selected_path_count(),
                entry.total_path_count(),
                if zh { "项已选择" } else { "selected" }
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
            self.fill_rounded_rect(all, chrome.close_background, chrome.preview_button_radius)?;
            self.stroke_rounded_rect(
                all,
                with_alpha(chrome.body_color, 0.12),
                chrome.preview_button_radius,
                1.0,
            )?;
            self.draw_text_no_wrap_with_style(
                if zh { "全选" } else { "All" },
                all,
                chrome.body_color,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
            )?;
            let none = smart_group_suggestor::suggestor_select_none_rect(viewport);
            self.fill_rounded_rect(none, chrome.close_background, chrome.preview_button_radius)?;
            self.stroke_rounded_rect(
                none,
                with_alpha(chrome.body_color, 0.12),
                chrome.preview_button_radius,
                1.0,
            )?;
            self.draw_text_no_wrap_with_style(
                if zh { "清空" } else { "None" },
                none,
                chrome.muted_color,
                10.0,
                550,
                1.0,
                dwrite::TextAlign {
                    h: dwrite::HAlign::Center,
                    v: dwrite::VAlign::Center,
                },
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
                    (true, true) => "› ✓",
                    (true, false) => "› ○",
                    (false, true) => "  ✓",
                    (false, false) => "  ○",
                };
                let label = smol_str::SmolStr::new(format!(
                    "{} {}",
                    marker,
                    smart_group_suggestor::path_basename(path)
                ));
                self.draw_text_no_wrap(
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

    fn current_logical_transform_matrix(&self) -> Matrix3x2 {
        self.logical_transform_override
            .unwrap_or_else(|| base_scale_matrix(self.base_scale.max(0.01)))
    }

    fn set_logical_transform_override(
        &mut self,
        transform: Option<Matrix3x2>,
    ) -> Result<(), RenderError> {
        self.logical_transform_override = transform;
        let current = self.current_logical_transform_matrix();
        let ctx = self.ctx()?;
        // SAFETY: the D2D context is inside a BeginDraw/EndDraw pair. The
        // matrix is stack-owned and copied by D2D for subsequent draw calls.
        unsafe {
            ctx.SetTransform(&current);
        }
        Ok(())
    }

    fn svg_fit_matrix_in_current_transform(
        &self,
        rect: bento_nano_style::Rect,
        view_size: f32,
    ) -> Matrix3x2 {
        let scale = (rect.width / view_size).min(rect.height / view_size);
        let glyph_w = view_size * scale;
        let glyph_h = view_size * scale;
        let dx = rect.x + (rect.width - glyph_w) * 0.5;
        let dy = rect.y + (rect.height - glyph_h) * 0.5;
        let logical = self.current_logical_transform_matrix();
        Matrix3x2 {
            M11: scale * logical.M11,
            M12: 0.0,
            M21: 0.0,
            M22: scale * logical.M22,
            M31: dx * logical.M11 + logical.M31,
            M32: dy * logical.M22 + logical.M32,
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

    /// Frosted-backdrop — build the per-frame `ID2D1BitmapBrush` from the cached
    /// blurred desktop snapshot. Returns `None` (→ flat-tint degrade) when there
    /// is no backdrop or any COM step fails; NEVER panics (spec § "Degrade
    /// ladder"). Called once per Main-overlay frame by `render()` (spec §10).
    ///
    /// Brush transform: the backdrop bitmap is captured at `region.top_left ==
    /// client logical (0,0)` (the Main overlay IS the primary work area), so the
    /// translation is `(0,0)`; the per-axis scale is
    /// `backdrop_brush_scale(downsample, base_scale) = downsample / base_scale`
    /// — see that helper's derivation. ExtendMode CLAMP both axes so the brush
    /// never tiles past the captured region; LINEAR interpolation for a smooth
    /// upscale of the downsampled source.
    fn build_backdrop_brush(
        &self,
        ctx: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
    ) -> Option<ID2D1BitmapBrush> {
        let backdrop = self.backdrop.as_ref()?;
        // Spec §15.1 — Interface::cast canonical for COM cross-cast; degrade on
        // failure rather than `?`-propagating a hard error out of the hot path.
        let rt: ID2D1RenderTarget = ctx.cast().ok()?;
        let props = D2D1_BITMAP_BRUSH_PROPERTIES {
            extendModeX: D2D1_EXTEND_MODE_CLAMP,
            extendModeY: D2D1_EXTEND_MODE_CLAMP,
            interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        };
        // SAFETY: rt valid for the call; `backdrop.bitmap` (`ID2D1Bitmap1`)
        //         derefs to the `ID2D1Bitmap` the brush wants; `props` lives on
        //         the stack for the call. `None` brush-properties = identity
        //         opacity + identity transform (we set the transform below).
        let brush = unsafe { rt.CreateBitmapBrush(&backdrop.bitmap, Some(&props), None) }.ok()?;
        let s = backdrop_brush_scale(FROSTED_BACKDROP_DOWNSAMPLE, self.base_scale);
        let transform = windows::Foundation::Numerics::Matrix3x2 {
            M11: s,
            M12: 0.0,
            M21: 0.0,
            M22: s,
            M31: 0.0,
            M32: 0.0,
        };
        // SAFETY: brush valid; `SetTransform` lives on the `ID2D1Brush` base
        //         (the bitmap brush derefs to it); `transform` lives for the
        //         call. Maps bitmap-px → pre-world DIP so the frost lands 1:1
        //         on the wallpaper after the world transform applies base_scale.
        unsafe {
            brush.SetTransform(&transform);
        }
        Some(brush)
    }

    /// Frosted-backdrop unified surface fill (spec § "Renderer plumbing"). When
    /// a per-frame backdrop brush exists, paint the blurred desktop CLIPPED to
    /// the rounded shape, then lay a SINGLE `tint` at the Tauri alpha on top —
    /// real frosted glass. With no brush (degrade / `FROSTED_BACKDROP` off) this
    /// is exactly `fill_rounded_rect(rect, tint, radius)`: one clean flat tint,
    /// NEVER the old double translucent layer (so the murk can never return).
    fn fill_frosted_rect(
        &self,
        rect: bento_nano_style::Rect,
        tint: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        // The baked capture can exist yet still be transparent on a driver that
        // rejects its effect output. A same-colour underlay guarantees the
        // final surface reaches the fallback opacity; a healthy opaque capture
        // simply covers it before the source Tauri tint is applied.
        if let Some(underlay) = frosted_fallback_underlay(tint) {
            self.fill_rounded_rect(rect, underlay, radius)?;
        }
        if let Some(brush) = self.backdrop_brush.as_ref() {
            if rect.width > 0.0 && rect.height > 0.0 {
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
                // SAFETY: rt valid; `rr` lives for the call; the bitmap brush is
                //         COM-ref-counted and was built for this frame's ctx.
                unsafe {
                    rt.FillRoundedRectangle(&rr, brush);
                }
            }
        }
        self.fill_rounded_rect(rect, tint, radius)
    }

    /// Apply CSS-like group opacity to the complete frosted surface. Fading
    /// only the tint leaves the captured desktop bitmap fully opaque, which
    /// makes stack emerge/bloom transitions look like a hard black slab.
    fn fill_frosted_rect_with_group_opacity(
        &self,
        rect: bento_nano_style::Rect,
        tint: Color,
        radius: BorderRadius,
        opacity: f32,
    ) -> Result<(), RenderError> {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        if opacity >= 1.0 - f32::EPSILON {
            return self.fill_frosted_rect(rect, tint, radius);
        }

        if let Some(brush) = self.backdrop_brush.as_ref() {
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
            let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
            let backdrop_opacity = frosted_group_backdrop_opacity(tint.a, opacity);
            // SAFETY: brush/rt are valid for this frame. Restore the shared
            // brush to identity opacity before any following surface uses it.
            unsafe {
                brush.SetOpacity(backdrop_opacity);
                rt.FillRoundedRectangle(&rr, brush);
                brush.SetOpacity(1.0);
            }
        }

        self.fill_rounded_rect(rect, fade_color(tint, opacity), radius)
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
    /// soft fill (the grow-and-fill idiom, no D2D blur effect on the hot path).
    /// Layers draw back-to-front so the inner surface lift sits under the
    /// dominant outer drop.
    ///
    /// #3 step 10 (2026-06-02) — each layer is FEATHERED instead of stamped as
    /// one crisp full-alpha rounded rect. A real CSS `box-shadow: 0 4px 16px ...`
    /// spreads its alpha across a 16–48px Gaussian gradient that is near-zero at
    /// the panel edge; the old single grow-and-fill put the full token alpha
    /// right up to a sharp rectangle boundary, so the expanded zone's 2-layer
    /// shadow read as a hard "extra border" ring ~16px outside the 1px hairline.
    /// We now paint `FEATHER_BANDS` concentric rects per layer, from the full
    /// grow (faint) inward toward the panel (denser): each band carries
    /// `per_band_alpha = 1 - (1 - A)^(1/N)`, so the N bands that overlap nearest
    /// the panel composite back UP to the token alpha `A` (0x33 / 0x66 kept
    /// EXACTLY), while the outer edge — covered by only the first band — fades to
    /// `per_band_alpha`, giving the soft blur falloff. A spread-only ring
    /// (`blur == 0`, e.g. `terminal`'s `0 0 0 1px`) keeps its single crisp fill.
    /// Allocation-free: a fixed stack-`f32` loop, reusing `fill_rounded_rect`
    /// (§10). An empty stack (`flat`/`brutalism`/`editorial`) is a no-op.
    fn fill_rounded_rect_vertical_gradient(
        &mut self,
        rect: bento_nano_style::Rect,
        top: Color,
        bottom: Color,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        self.fill_rounded_rect_linear_gradient(
            rect,
            top,
            bottom,
            radius,
            vertical_gradient_props(rect),
        )
    }

    fn fill_rounded_rect_linear_gradient(
        &mut self,
        rect: bento_nano_style::Rect,
        start: Color,
        end: Color,
        radius: BorderRadius,
        props: D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    ) -> Result<(), RenderError> {
        if (start.a <= 0.0 && end.a <= 0.0) || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let brush = self.linear_gradient_brush(props, start, end)?;
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
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        unsafe {
            rt.FillRoundedRectangle(&rr, &brush);
        }
        Ok(())
    }

    fn linear_gradient_brush(
        &mut self,
        props: D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        top: Color,
        bottom: Color,
    ) -> Result<ID2D1LinearGradientBrush, RenderError> {
        let needs_rebuild = match self.linear_gradient_brush.as_ref() {
            Some(cached) => cached.top != top || cached.bottom != bottom,
            None => true,
        };
        if needs_rebuild {
            let stops = [d2d_gradient_stop(0.0, top), d2d_gradient_stop(1.0, bottom)];
            let ctx = self.ctx()?;
            let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
            let stop_collection = ok("CreateGradientStopCollection", unsafe {
                rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
            })?;
            let brush = ok("CreateLinearGradientBrush", unsafe {
                rt.CreateLinearGradientBrush(&props, None, &stop_collection)
            })?;
            self.linear_gradient_brush = Some(CachedLinearGradientBrush { top, bottom, brush });
        }
        let Some(cached) = self.linear_gradient_brush.as_ref() else {
            return Err(RenderError::Platform(
                bento_nano_platform::PlatformError::Init(
                    "Renderer: gradient brush cache missing after rebuild",
                ),
            ));
        };
        let brush = cached.brush.clone();
        unsafe {
            brush.SetStartPoint(props.startPoint);
            brush.SetEndPoint(props.endPoint);
        }
        Ok(brush)
    }

    fn draw_shadow_stack(
        &self,
        base: bento_nano_style::Rect,
        stack: bento_nano_style::ShadowStack,
        radius: BorderRadius,
    ) -> Result<(), RenderError> {
        // W13-B (2026-07-13) — a blurred CSS shadow cannot be approximated by
        // repeatedly filling larger opaque geometry. The former twenty-band
        // painter produced the broad black/gray cloud visible in the user's
        // hand-test and multiplied every Zone paint by up to forty fills.
        // Preserve authored zero-blur outline/ring layers (e.g. stack preview)
        // and suppress blur layers until a real native effect is justified.
        for layer in stack.layers() {
            if let Some(rect) = crisp_shadow_rect(base, *layer) {
                self.fill_rounded_rect(rect, layer.color, radius)?;
            }
        }
        Ok(())
    }

    // =========================================================================
    // M6c — the 3 effect render primitives + the post-pass dispatcher.
    //
    // All read `app.active_theme_effect_tauri()` (`Copy`, §10) and no-op for
    // `EffectTauri::None`, so the 14 non-effect themes pay nothing. The blur
    // neon house-style is grow-and-fill (NOT `CLSID_D2D1Shadow`); ordinary
    // box-shadow blur layers are intentionally suppressed by W13-B. GPU draw
    // itself is verified by the §6 visual
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

    /// Paint the selected-stack expanded panel `border-top` as CSS does: stroke
    /// the full rounded border, then clip that stroke to the top 2-DIP strip.
    /// The old inner filled slab was inset by the full corner radius, so it read
    /// like a second border inside the panel instead of the panel's own top edge.
    fn draw_expanded_panel_accent_edge(
        &self,
        rect: bento_nano_style::Rect,
        radius: BorderRadius,
        accent: Color,
    ) -> Result<(), RenderError> {
        if accent.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let clip = expanded_panel_accent_clip_rect(rect);
        if clip.width <= 0.0 || clip.height <= 0.0 {
            return Ok(());
        }
        self.push_clip(clip)?;
        let result = self.stroke_rounded_rect(rect, accent, radius, PANEL_ACCENT_EDGE_THICKNESS_PX);
        let pop_result = self.pop_clip();
        result.and(pop_result)
    }

    /// G5 (2026-06-01) — stroke a rounded-rect outline with a DASHED hairline.
    /// Used for the collapsed `minimal`-shape capsule, whose Tauri chrome is
    /// `border: 1px dashed rgba(255,255,255,0.2)` over a transparent body
    /// (`BentoZone.css:92-99 .bento-zone--shape-minimal`). The dash cadence is
    /// the predefined `D2D1_DASH_STYLE_DASH` (2 on / 2 off in stroke-width
    /// units), which reads as a clean CSS-style dashed edge at the 1-DIP width.
    ///
    /// §10: the `ID2D1StrokeStyle` is built ONCE per process and cached in a
    /// `OnceLock` (it is created from the device-INDEPENDENT D2D factory, so it
    /// survives device-loss rebuilds and never re-allocates per frame). §11: no
    /// panic/unwrap — the build is `?`-propagated, the cache uses `get_or_init`
    /// with a fallible inner that falls back to a solid stroke on any error.
    fn stroke_rounded_rect_dashed(
        &mut self,
        rect: bento_nano_style::Rect,
        color: Color,
        radius: BorderRadius,
        stroke_width: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::Direct2D::{
            D2D1_CAP_STYLE_FLAT, D2D1_DASH_STYLE_DASH, D2D1_LINE_JOIN_MITER,
            D2D1_STROKE_STYLE_PROPERTIES, ID2D1Factory,
        };
        if color.a <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 || stroke_width <= 0.0 {
            return Ok(());
        }
        // Lazily build + cache the dashed stroke style on the renderer. It is
        // created from the device-INDEPENDENT D2D factory, so the cached handle
        // stays valid across device-loss rebuilds and re-skins; one COM
        // allocation per process, ZERO per frame (§10). Single-threaded UI
        // renderer, so a plain `Option` field is the right cache, not a global
        // (`ID2D1StrokeStyle` is not `Sync`).
        if self.dashed_stroke_style.is_none() {
            let d2d = d2d::factory()?;
            // Cross-cast `ID2D1Factory1` → base `ID2D1Factory` (§15.1 canonical)
            // so `CreateStrokeStyle` resolves to the base overload that takes
            // `D2D1_STROKE_STYLE_PROPERTIES` and returns `ID2D1StrokeStyle`
            // (the `Factory1` overload wants `..._PROPERTIES1`/`...Style1`).
            let factory: ID2D1Factory = ok("Factory1::cast<Factory>", d2d.factory.cast())?;
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_FLAT,
                endCap: D2D1_CAP_STYLE_FLAT,
                dashCap: D2D1_CAP_STYLE_FLAT,
                lineJoin: D2D1_LINE_JOIN_MITER,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_DASH,
                dashOffset: 0.0,
            };
            // SAFETY: `props` lives for the call; `dashes: None` selects the
            // predefined DASH cadence; the returned style is COM-ref-counted.
            let style = ok("CreateStrokeStyle", unsafe {
                factory.CreateStrokeStyle(&props, None)
            })?;
            self.dashed_stroke_style = Some(style);
        }
        let dash_style = self.dashed_stroke_style.as_ref();
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
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: rt valid; rr + dash_style live for the call; brush COM-ref-counted.
        unsafe {
            rt.DrawRoundedRectangle(&rr, &brush, stroke_width, dash_style);
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
            D2D_SIZE_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
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
            size: D2D_SIZE_F {
                width: r,
                height: r,
            },
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
        let geom = ok("CreatePathGeometry", unsafe {
            factory.CreatePathGeometry()
        })?;
        let sink: ID2D1GeometrySink = ok("PathGeometry::Open", unsafe { geom.Open() })?;
        // Walk the perimeter clockwise from the top edge, arcing rounded
        // corners and cutting straight to the geometric corner on square ones.
        // SAFETY: sink valid until Close() below; all points live on the stack.
        unsafe {
            sink.BeginFigure(D2D_POINT_2F { x: l + tl, y: t }, D2D1_FIGURE_BEGIN_FILLED);
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
        self.draw_text_aligned(text, rect, color, dwrite::TextAlign::DEFAULT)
    }

    /// #1 step 13 (2026-06-02) — single text drawing entry point with explicit
    /// DWrite alignment. Default text still flows through [`draw_text`], while
    /// icon/glyph fallbacks and other centred chips pass a non-default
    /// [`dwrite::TextAlign`]. This keeps the old isolated `draw_text_centered`
    /// path folded into the same layout builder as every other text run.
    fn draw_text_aligned(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        align: dwrite::TextAlign,
    ) -> Result<(), RenderError> {
        let format = self.text_format.clone();
        self.draw_text_with_format(text, rect, color, &format, align)
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
            dwrite::TextAlign::DEFAULT,
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    fn draw_settings_text(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_LABEL_SIZE,
            crate::settings_panel::SETTINGS_TEXT_LABEL_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
        )
    }

    fn draw_settings_group_title(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_tracked(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_GROUP_TITLE_SIZE,
            crate::settings_panel::SETTINGS_GROUP_TITLE_WEIGHT,
            crate::settings_panel::SETTINGS_GROUP_TITLE_TRACKING,
        )
    }

    fn draw_settings_text_no_wrap(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
            crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
            dwrite::TextAlign::DEFAULT,
        )
    }

    fn draw_settings_button_text(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size: f32,
        weight: u16,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            size,
            weight,
            1.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Center,
            },
        )
    }

    fn draw_settings_row_value(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style(
            text,
            rect,
            color,
            crate::settings_panel::SETTINGS_TEXT_VALUE_SIZE,
            crate::settings_panel::SETTINGS_TEXT_VALUE_WEIGHT,
            crate::settings_panel::SETTINGS_TEXT_LINE_HEIGHT,
            dwrite::TextAlign {
                h: dwrite::HAlign::Trailing,
                v: dwrite::VAlign::Center,
            },
        )
    }

    /// #7 §10 parity (2026-06-01) — no-wrap variant of [`draw_text_with_style`].
    ///
    /// `draw_text_with_style` routes through `CreateTextLayout`, which leaves
    /// DWrite's default word-wrapping ON and creates a layout object for every
    /// short label. StackTray/Settings fixed chips are many small single-line
    /// runs; building a layout for each one caused a large DirectWrite private
    /// heap jump on first StackTray open. This helper keeps the cached per-style
    /// `IDWriteTextFormat`, temporarily applies NO_WRAP + explicit alignment,
    /// then uses `ID2D1RenderTarget::DrawText` with clipping. The format is reset
    /// to default wrapping/alignment immediately after the draw so shared cached
    /// formats do not leak state into the regular layout path.
    ///
    /// The old styled path used `sign: None`, so overflow was already a silent
    /// trim. `DrawText` + `D2D1_DRAW_TEXT_OPTIONS_CLIP` preserves that "fit in
    /// one line, clipped if necessary" contract without per-label layout COM
    /// allocation.
    ///
    /// §10: reuses `utf16_scratch` (cleared, never freed) and the bounded format
    /// cache; no new dependency or unbounded text cache.
    /// #1 step 12/13 (2026-06-02) — `align` sets the DWrite text/paragraph
    /// alignment for the run. The origin stays the rect's top-left, so a
    /// `Center` horizontal alignment centres the run WITHIN `rect.width` (the
    /// item-card label centring under its icon) and a `Center` vertical
    /// alignment centres WITHIN `rect.height` (the header title / count badge,
    /// exact instead of the old `(band - size*1.4)/2` baseline approximation).
    /// Pass [`dwrite::TextAlign::DEFAULT`] for the legacy Leading/Near top-left.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_no_wrap_with_style(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
        align: dwrite::TextAlign,
    ) -> Result<(), RenderError> {
        self.draw_text_no_wrap_with_style_transformed(
            text,
            rect,
            color,
            size_pt,
            weight,
            line_height,
            align,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_no_wrap_with_style_transformed(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        size_pt: f32,
        weight: u16,
        line_height: f32,
        align: dwrite::TextAlign,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(size_pt, weight, line_height)?;
        let brush = self.solid_brush(color)?;
        let layout_rect = D2D_RECT_F {
            left: rect.x,
            top: rect.y,
            right: rect.right(),
            bottom: rect.bottom(),
        };
        let ctx = self.ctx()?;
        let rt: ID2D1RenderTarget = ok("DeviceContext::cast<RenderTarget>", ctx.cast())?;
        // SAFETY: `format` is a live DWrite COM object. Set* mutates only this
        // cached format's draw properties; we reset them below before returning.
        unsafe {
            ok(
                "StackText.SetTextAlignment",
                format.SetTextAlignment(direct_text_halign(align)),
            )?;
            ok(
                "StackText.SetParagraphAlignment",
                format.SetParagraphAlignment(direct_text_valign(align)),
            )?;
            ok(
                "StackText.SetWordWrapping",
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP),
            )?;
        }
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        // SAFETY: rt/format/brush are live COM interfaces. `utf16_scratch` and
        // `layout_rect` live for the call, and DrawText does not retain them.
        unsafe {
            if let Some(transform) = draw_transform.as_ref() {
                rt.SetTransform(transform);
            }
            rt.DrawText(
                &self.utf16_scratch,
                &format,
                &layout_rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            if draw_transform.is_some() {
                let base = self.current_logical_transform_matrix();
                rt.SetTransform(&base);
            }
            ok(
                "StackText.ResetTextAlignment",
                format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING),
            )?;
            ok(
                "StackText.ResetParagraphAlignment",
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR),
            )?;
            ok(
                "StackText.ResetWordWrapping",
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP),
            )?;
        }
        Ok(())
    }

    /// Draw full item labels with no wrapping and no generated ellipsis.
    /// Tauri ItemCard's `useTextAbbrGroup` keeps the complete display name and
    /// shrinks the font size toward 8px instead of substituting `...`.
    fn draw_item_label_no_wrap(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        font_px: f32,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(font_px, 400, 1.4)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            None,
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Near,
            },
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// Draw stack bloom petal names with the native two-line clamp budget.
    ///
    /// The geometry layer supplies the fixed two-line title slot; this draw path
    /// keeps DWrite wrapping enabled and applies character trimming with an
    /// ellipsis sign only when the text exceeds that slot.
    fn draw_stack_bloom_petal_name(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(
            stack_tray::BLOOM_PETAL_NAME_FONT_PX,
            stack_tray::BLOOM_PETAL_NAME_FONT_WEIGHT,
            stack_tray::BLOOM_PETAL_NAME_LINE_HEIGHT,
        )?;
        if self.bloom_petal_ellipsis_sign.is_none() {
            self.bloom_petal_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        let trim_sign = self.bloom_petal_ellipsis_sign.clone();
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_wrapped_trimmed(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            trim_sign.as_ref(),
            dwrite::TextAlign {
                h: dwrite::HAlign::Center,
                v: dwrite::VAlign::Near,
            },
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// G5 (2026-06-01) — measure the laid-out width of `text` at the given style
    /// (no-wrap, single line) via `IDWriteTextLayout::GetMetrics`. Returns the
    /// `widthIncludingTrailingWhitespace` in DIPs. Used by the stack-capsule
    /// title shrink path. Reuses the cached
    /// per-style format from the LRU + the `utf16_scratch` buffer, so a measure
    /// allocates nothing on the heap (§10). A measure layout is built with a
    /// generous `max_w` so the metric reflects the natural (unclamped) run width.
    fn measure_label_width(
        &mut self,
        text: &str,
        size_pt: f32,
        weight: u16,
    ) -> Result<f32, RenderError> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
        let format = self.text_format_for_style(size_pt, weight, 1.0)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        // Large max_w so NO_WRAP measurement returns the intrinsic run width.
        // Alignment is irrelevant to width measurement → DEFAULT.
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            f32::MAX,
            64.0,
            None,
            dwrite::TextAlign::DEFAULT,
        )?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: layout is a freshly-created COM interface; GetMetrics writes
        // the out-struct and returns HRESULT only on catastrophic error.
        ok("TextLayout::GetMetrics", unsafe {
            layout.GetMetrics(&mut metrics)
        })?;
        Ok(metrics.widthIncludingTrailingWhitespace)
    }

    /// Draw a collapsed-pill title at the stable readable typography role.
    /// DWrite performs single-line character trimming with an inline ellipsis;
    /// capsule size changes available width, never the perceived text scale.
    fn draw_pill_title_ellipsis(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        font_px: f32,
        tracking_px: f32,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};

        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let format = self.text_format_for_style(font_px, 500, 1.0)?;
        if self.pill_title_ellipsis_sign.is_none() {
            self.pill_title_ellipsis_sign = Some(dwrite::create_ellipsis_sign(&format)?);
        }
        self.utf16_scratch.clear();
        self.utf16_scratch.extend(text.encode_utf16());
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            self.pill_title_ellipsis_sign.as_ref(),
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
        )?;
        if tracking_px.abs() > f32::EPSILON {
            let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: self.utf16_scratch.len() as u32,
            };
            // SAFETY: layout1 is private to this draw and the range covers the
            // exact UTF-16 source run retained in `utf16_scratch`.
            unsafe {
                let _ = layout1.SetCharacterSpacing(0.0, tracking_px, 0.0, range);
            }
        }
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        // SAFETY: layout and brush remain alive for the immediate D2D call.
        unsafe {
            ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    /// V21-C6 (2026-06-22) — Tauri `StackCapsule` also delegates its title to
    /// `useTextAbbr`: the grid column owns the width, and the label shrinks
    /// before it visually truncates. nano previously drew stack titles at a
    /// fixed 13px/600, producing `"Benchmark..."` in the 220px two-member
    /// capsule. This shares the ordinary capsule shrink path while preserving
    /// StackCapsule's typography token (13px / 600 / centered line box).
    fn draw_stack_capsule_title_shrink_to_fit_transformed(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        self.draw_title_shrink_to_fit(
            text,
            rect,
            color,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
            1.2,
            0.0,
            dwrite::TextAlign {
                h: dwrite::HAlign::Leading,
                v: dwrite::VAlign::Center,
            },
            draw_transform,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_title_shrink_to_fit(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        base_px: f32,
        weight: u16,
        line_height: f32,
        tracking: f32,
        align: dwrite::TextAlign,
        draw_transform: Option<windows::Foundation::Numerics::Matrix3x2>,
    ) -> Result<(), RenderError> {
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};
        if text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let avail_w = rect.width;
        let sig = title_shrink_signature(text, avail_w, base_px, weight, tracking);
        // --- Resolve the fit font size (cache → measure-and-shrink) ---------
        let resolved_px = match self.stack_capsule_title_shrink {
            Some((cached_sig, px)) if cached_sig == sig => px,
            _ => {
                // Miss: step the font down until it fits (or hit the floor) via
                // the shared pure `shrink_font_to_fit` stepper. The `measure`
                // closure threads any DWrite error out through `measure_err` so
                // the loop stays panic-free (§11); a measure failure short-
                // circuits the stepper to the floor and is surfaced below.
                let mut measure_err: Option<RenderError> = None;
                let utf16_units = text.encode_utf16().count();
                let resolved = shrink_font_to_fit(base_px, avail_w, |size| {
                    if measure_err.is_some() {
                        // Already failed — report "fits" so the stepper stops
                        // immediately at the current size; the error wins below.
                        return 0.0;
                    }
                    match self.measure_label_width(text, size, weight) {
                        Ok(w) => text_width_with_tracking(w, utf16_units, tracking),
                        Err(e) => {
                            measure_err = Some(e);
                            0.0
                        }
                    }
                });
                if let Some(e) = measure_err {
                    return Err(e);
                }
                self.stack_capsule_title_shrink = Some((sig, resolved));
                resolved
            }
        };
        let format = self.text_format_for_style(resolved_px, weight, line_height)?;
        self.utf16_scratch.clear();
        for u in text.encode_utf16() {
            self.utf16_scratch.push(u);
        }
        let layout = dwrite::create_layout_no_wrap(
            &self.utf16_scratch,
            &format,
            rect.width.max(1.0),
            rect.height.max(1.0),
            None,
            align,
        )?;
        let brush = self.solid_brush(color)?;
        let origin = D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        };
        let ctx = self.ctx()?;
        if tracking.abs() > f32::EPSILON {
            // Letter-spacing via IDWriteTextLayout1 (§15.1 canonical cast).
            let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: self.utf16_scratch.len() as u32,
            };
            // SAFETY: layout1 is freshly created; SetCharacterSpacing only mutates
            // per-instance spacing over the canonical full range.
            unsafe {
                let _ = layout1.SetCharacterSpacing(0.0, tracking, 0.0, range);
                if let Some(transform) = draw_transform.as_ref() {
                    ctx.SetTransform(transform);
                }
                ctx.DrawTextLayout(origin, &layout1, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                if draw_transform.is_some() {
                    let base = self.current_logical_transform_matrix();
                    ctx.SetTransform(&base);
                }
            }
        } else {
            // SAFETY: ctx valid; layout owned for the call; brush COM-ref-counted.
            unsafe {
                if let Some(transform) = draw_transform.as_ref() {
                    ctx.SetTransform(transform);
                }
                ctx.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                if draw_transform.is_some() {
                    let base = self.current_logical_transform_matrix();
                    ctx.SetTransform(&base);
                }
            }
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
        self.draw_text_with_format(text, rect, color, &format, dwrite::TextAlign::DEFAULT)
    }

    fn draw_text_with_format(
        &mut self,
        text: &str,
        rect: bento_nano_style::Rect,
        color: Color,
        format: &IDWriteTextFormat,
        align: dwrite::TextAlign,
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
            align,
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
        if let Some(index) = self.text_format_cache.iter().position(|cached| {
            cached.family == self.text_format_family
                && (cached.size_pt - size_pt).abs() < f32::EPSILON
                && cached.weight == weight
                && (cached.line_height - line_height).abs() < f32::EPSILON
        }) {
            let format = self.text_format_cache[index].format.clone();
            if index + 1 < self.text_format_cache.len() {
                let entry = self.text_format_cache.remove(index);
                self.text_format_cache.push(entry);
            }
            return Ok(format);
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
            self.text_format_cache.remove(0);
        }
        self.text_format_cache.push(entry);
        Ok(format)
    }

    /// M1i fidelity (2026-05-29) — lazily create/cache the monospace text
    /// format for the §2 source-card path line. Tauri's `.desktop-source-card
    /// __path` uses `font-family: ui-monospace, Consolas, monospace`; Consolas
    /// is the Win10/11 fixed-pitch system font (no bundled `.ttf`, spec §5).
    /// `size_pt` is the path font size in DIP (11). Cached against the size so
    /// a theme swap (which only touches the proportional body font) never
    /// invalidates it. One COM allocation per recreate, zero per frame.
    fn ensure_monospace_format(&mut self, size_pt: f32) -> Result<IDWriteTextFormat, RenderError> {
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
            dwrite::TextAlign::DEFAULT,
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
        use windows::Win32::Graphics::DirectWrite::{DWRITE_TEXT_RANGE, IDWriteTextLayout1};
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
            dwrite::TextAlign::DEFAULT,
        )?;
        // SetCharacterSpacing lives on IDWriteTextLayout1 — cross-cast per
        // spec §15.1 (canonical Interface::cast). Apply `tracking` as the
        // trailing advance over the entire glyph run; leading + min-advance 0.
        let layout1: IDWriteTextLayout1 = ok("TextLayout::cast<TextLayout1>", layout.cast())?;
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
        // 1:1 translate — the path is already the right size. Compose against
        // the current logical transform so grouped animations preserve icons.
        let logical = self.current_logical_transform_matrix();
        let m = Matrix3x2 {
            M11: logical.M11,
            M12: 0.0,
            M21: 0.0,
            M22: logical.M22,
            M31: rect.x * logical.M11 + logical.M31,
            M32: rect.y * logical.M22 + logical.M32,
        };
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the current logical transform so subsequent draw calls
            // stay in the grouped surface animation.
            let base = self.current_logical_transform_matrix();
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
        let m = self.svg_fit_matrix_in_current_transform(rect, view_size);
        // SAFETY: ctx valid; brush + geom outlive the call; matrix on stack.
        unsafe {
            ctx.SetTransform(&m);
            ctx.FillGeometry(&geom, &brush, None);
            // Restore the current logical transform so grouped surface
            // animations continue after the per-glyph transform.
            let base = self.current_logical_transform_matrix();
            ctx.SetTransform(&base);
        }
        Ok(())
    }

    /// RC-4 Gap 1 — render a zone-icon name as a real line-art glyph.
    ///
    /// `name` is the wire-format icon string from `Zone.icon` (e.g. "folder",
    /// "settings", "search"). When it resolves to a built-in `IconKind`, the
    /// matching 24×24 source SVG document is drawn via
    /// `draw_svg_document_stroke_fit` (cached geometry). Unknown or legacy text
    /// payloads deliberately render as a neutral built-in glyph instead of
    /// visible emoji/text placeholders.
    fn draw_icon_glyph(
        &mut self,
        name: &str,
        rect: bento_nano_style::Rect,
        color: Color,
    ) -> Result<(), RenderError> {
        if !zone_pill_geometry::icon_name_has_visible_glyph(name) {
            return Ok(());
        }
        if let Some(kind) = IconKind::from_str_opt(name) {
            // 24-unit viewbox per `IconKind::source_svg` — every built-in is
            // hand-rolled around 0–24 just like the 1.x Tauri sources.
            // `draw_svg_document_stroke_fit` already h+v-centres the glyph in
            // `rect` (scale-to-fit + 0.5 offset).
            return self.draw_svg_document_stroke_fit(kind.source_svg(), rect, color, 24.0);
        }
        // No-emoji runtime policy (2026-06-18): keep wire compatibility for
        // old layouts that store arbitrary text/emoji icon payloads, but never
        // paint those payloads as UI icons.
        self.draw_svg_document_stroke_fit(IconKind::Document.source_svg(), rect, color, 24.0)
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
        let m = self.svg_fit_matrix_in_current_transform(rect, view_size);
        // SAFETY: rt valid; geometry and brush are COM references alive for
        // the call; matrix lives on the stack; `None` uses D2D's default
        // round-cap/round-join behavior encoded by the source line art.
        unsafe {
            rt.SetTransform(&m);
            rt.DrawGeometry(&geom, &brush, 1.5, None);
            let base = self.current_logical_transform_matrix();
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

/// P0 click-through shadow/glow margin in logical DIP.
///
/// Each chrome rect is inflated by this amount before the window region is
/// built so soft drop-shadows / hover glows are NOT hard-clipped by the OS at
/// the region edge (which would read as a sharp rectangular cut through the
/// shadow). Derived from the dominant painted pill shadow
/// (`SHADOW.zen.outer()`: `offset_y 8 + blur 32`): the visible falloff reaches
/// roughly `offset + blur/2 = 8 + 16 = 24` DIP past the surface. The expanded
/// panel's larger shadow (`offset 16 + blur 48`) extends further but is purely
/// decorative — a faint clip there is acceptable, whereas widening the region
/// to its full 64-DIP reach would re-arm the desktop to catch clicks well
/// outside the visible panel. 24 DIP is the balance: covers the common pill
/// shadow fully, keeps the click-through margin tight.
const CHROME_REGION_SHADOW_MARGIN_DIP: f32 = 24.0;

/// P0 click-through (CLICKTHROUGH-FIX-VALIDATED.md, 2026-06-02) — the union of
/// every currently-PAINTED interactive surface on the Main overlay, in logical
/// DIP. This is the single source of truth for the Main HWND window region (see
/// [`Renderer::apply_main_click_through_region`]): blank areas fall OUTSIDE the
/// region so clicks reach the desktop natively, painted chrome stays
/// interactive.
///
/// The set MUST mirror `bento-nano-shell::ui::main_nchittest_kind` (which
/// classifies each client point `Client`/`Caption` vs `Transparent`): every
/// rect here corresponds to a case where that fn returns NON-`Transparent`.
/// Item cards, resize corners, and `PanelHeader` buttons are all geometric
/// SUBSETS of their owning zone's body/pill rect, so unioning the zone rects
/// already covers them — no need to enumerate the sub-rects.
///
/// Each rect is inflated by [`CHROME_REGION_SHADOW_MARGIN_DIP`]. Pure /
/// allocation-lean: one stack `SmallVec`, no heap beyond a spill on a very
/// large zone count. Returns rects in DIP; the caller converts to physical px.
fn chrome_region_rects(app: &AppState) -> SmallVec<[bento_nano_style::Rect; 16]> {
    use bento_nano_style::Rect;
    let mut out: SmallVec<[Rect; 16]> = SmallVec::new();
    let vp = app.viewport;
    let full = Rect {
        x: 0.0,
        y: 0.0,
        width: vp.width.max(0.0),
        height: vp.height.max(0.0),
    };

    // Any in-flight drag/resize routes EVERY
    // point to `Client` so the gesture keeps receiving moves even over blank
    // desktop. Cover the full viewport for the duration of the drag.
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
        || app.stack_tray_drag.get().is_some()
    {
        push_inflated(&mut out, full, 0.0);
        return out;
    }

    // Stack overlay (open tray + focused preview, or a hovered-anchor bloom).
    // Mirrors `ui::stack_overlay_contains`.
    push_stack_overlay_rects(app, &mut out, full);

    // App-rendered context menu sits above zones on the Main surface. Include
    // its compact bounding box (including the submenu bridge) without the Zone
    // shadow inflation so every visible row receives production pointer input.
    if let Some(session) = app.active_context_menu.borrow().as_ref() {
        push_clamped_inflated(&mut out, popover::context_menu_bounds(session), full, 0.0);
    }

    // Per-zone painted surface — pill / in-flight morph / expanded body.
    // Mirrors `ui::effective_zone_hit_rect` + the `hit_test_zone` visibility
    // filter (skip hidden zones + stacked children).
    for zone in app.zones.iter() {
        if !zone.is_visible() || zone.is_stacked_child() {
            continue;
        }
        let rect = effective_zone_chrome_rect(app, zone);
        // Belt-and-suspenders (ROOT-CAUSE-corrupt-zone-geometry.md): clamp the
        // ZONE BODY rect to the viewport BEFORE inflating so an oversized /
        // corrupt zone can never make the whole window catch clicks. The
        // shadow-margin inflate may then extend slightly past the viewport —
        // that's fine (only the painted soft shadow), but the body itself can
        // never exceed the window region.
        push_clamped_inflated(&mut out, rect, full, CHROME_REGION_SHADOW_MARGIN_DIP);
    }

    out
}

/// Intersect `rect` with `bounds` (both DIP). Returns the overlapping rectangle,
/// or `None` when they do not overlap (or the intersection is degenerate). Pure /
/// allocation-free — the click-through region clamp depends on this so an
/// oversized zone can never push a rect beyond the window.
#[inline]
fn intersect_with_viewport(
    rect: bento_nano_style::Rect,
    bounds: bento_nano_style::Rect,
) -> Option<bento_nano_style::Rect> {
    let left = rect.x.max(bounds.x);
    let top = rect.y.max(bounds.y);
    let right = rect.right().min(bounds.right());
    let bottom = rect.bottom().min(bounds.bottom());
    if right <= left || bottom <= top {
        return None;
    }
    Some(bento_nano_style::Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Clamp `rect` to the viewport `bounds`, THEN inflate by `margin` and push it
/// (mirrors [`push_inflated`] but with the pre-inflate viewport clamp). A rect
/// fully outside the viewport is dropped entirely. The skip-degenerate guard in
/// `push_inflated` still applies because the clamped rect is forwarded through it.
#[inline]
fn push_clamped_inflated(
    out: &mut SmallVec<[bento_nano_style::Rect; 16]>,
    rect: bento_nano_style::Rect,
    bounds: bento_nano_style::Rect,
    margin: f32,
) {
    if let Some(clamped) = intersect_with_viewport(rect, bounds) {
        push_inflated(out, clamped, margin);
    }
}

/// Painted chrome rect for one zone — the DIP rectangle the renderer is
/// currently drawing. Re-implements `bento-nano-shell::ui::effective_zone_hit_rect`
/// in the `bento-nano-app` layer (the shell depends on app, not the reverse, so
/// the helper can't be imported; both sides consume the same `zone_pill_geometry`
/// SSoT so they stay in lockstep). Three cases: pill-morph in flight, collapsed
/// pill, expanded body. Pure / allocation-free.
fn effective_zone_chrome_rect(app: &AppState, zone: &Zone) -> bento_nano_style::Rect {
    use bento_nano_style::Rect;
    // #4 / R1 (2026-06-02) — a stack anchor's body is visible only when it is
    // explicitly selected (a focused member), NOT on hover (hover shows the
    // bloom). #5 (2026-06-02) — only a RESIZE (armable solely on an already-
    // expanded panel) may force the expanded body; a DRAG keeps a collapsed pill
    // a pill. Both rules now live in the shared `AppState::zone_pill_body_visible`
    // SSoT, the SAME predicate the paint side (`draw_zones`) and the z-layering
    // (`zone_on_top`) key off, so paint == hit geometry can't drift.
    let body_visible = app.zone_pill_body_visible(zone);
    let stack_member_count = app.zones.stack_member_ids(zone.id).map(|m| m.len());
    let count = stack_member_count.unwrap_or_else(|| zone.items.len());
    let pill_layout = zone_pill_geometry::pill_layout_for_zone(zone, count);
    let expanded_rect = Rect {
        x: zone.x as f32,
        y: zone.y as f32,
        width: zone.w as f32,
        height: zone.h as f32,
    };

    // Case 1 — pill morph in flight (mirrors effective_zone_hit_rect case 1).
    // Anchors don't morph (the paint-side pill_anim_active also excludes them).
    // #2 step 8 (2026-06-02) — shared `current_morph_rect` SSoT so paint == hit.
    if app.zone_pill_morph_in_flight(zone) {
        let raw = app.zone_pill_anim_progress.get();
        let (_morph, rect) = zone_pill_geometry::current_morph_rect(
            pill_layout.rect,
            expanded_rect,
            app.zone_pill_anim_from_morph.get(),
            raw,
            app.zone_pill_anim_expanding.get(),
        );
        return rect;
    }

    if !body_visible {
        if let Some(member_count) = stack_member_count {
            return zone_pill_geometry::stack_capsule_layout_for_zone(zone, member_count).rect;
        }
        return pill_layout.rect;
    }

    // Case 3 — expanded body (focused stack member uses the normal panel).
    expanded_rect
}

/// Push the stack-overlay chrome rects (open tray + focused preview, or a
/// hovered-anchor bloom) into `out`. Mirrors `ui::stack_overlay_contains` so
/// the region covers exactly the points that function returns `Client` for.
fn push_stack_overlay_rects(
    app: &AppState,
    out: &mut SmallVec<[bento_nano_style::Rect; 16]>,
    full: bento_nano_style::Rect,
) {
    let vp = app.viewport;
    // Open tray — tray body plus the focused preview pane only after a real
    // member is selected; the default anchor management view stays compact.
    if let Some(state) = app.stack_tray.borrow().clone() {
        if let Some(anchor) = app.zones.get(state.anchor_zone_id) {
            if let Some(members) = app.zones.stack_member_ids(anchor.id) {
                let member_count = members.len();
                if state.is_management() {
                    let tray = stack_tray::stack_tray_rect(vp, anchor, member_count);
                    push_clamped_inflated(out, tray, full, CHROME_REGION_SHADOW_MARGIN_DIP);
                    let selected_id = if members.contains(&state.selected_member_id) {
                        state.selected_member_id
                    } else {
                        members[0]
                    };
                    if stack_tray::focused_preview_visible(anchor.id, selected_id) {
                        push_clamped_inflated(
                            out,
                            stack_tray::focused_preview_rect(vp, tray),
                            full,
                            CHROME_REGION_SHADOW_MARGIN_DIP,
                        );
                    }
                } else if let Some(member_index) = members
                    .iter()
                    .position(|member_id| *member_id == state.selected_member_id)
                    && let Some(preview_zone) = app.zones.get(state.selected_member_id)
                {
                    let petals = stack_tray::stack_bloom_petal_rects(vp, anchor, member_count);
                    if let Some(petal) = petals.get(member_index).copied() {
                        push_clamped_inflated(
                            out,
                            stack_tray::focused_bloom_preview_rect(
                                vp,
                                petal,
                                &petals,
                                preview_zone,
                            ),
                            full,
                            CHROME_REGION_SHADOW_MARGIN_DIP,
                        );
                    }
                }
            }
        }
    }

    // Hovered-anchor bloom — the fan of petal rects shown while the cursor is
    // over a stack anchor. #4 / R1 (2026-06-02): mirror the render-side gate so
    // the click-through region never registers petal hit targets on a frame
    // where the bloom is NOT painted (tray open or a member focused/selected) —
    // no invisible dead click targets.
    let bloom_allowed = stack_surface_allows_bloom(app);
    if let Some(anchor_id) = app.stack_bloom_anchor.get().filter(|_| bloom_allowed) {
        if let Some(anchor) = app.zones.get(anchor_id) {
            if let Some(members) = app.zones.stack_member_ids(anchor.id) {
                let petals = if app.stack_bloom_leaving.get()
                    && app.stack_bloom_anchor.get() == Some(anchor.id)
                {
                    stack_tray::stack_bloom_exit_petal_rects_at(
                        vp,
                        anchor,
                        members.len(),
                        app.stack_bloom_progress.get(),
                    )
                } else {
                    stack_tray::stack_bloom_petal_rects(vp, anchor, members.len())
                };
                for petal in petals {
                    push_clamped_inflated(out, petal, full, CHROME_REGION_SHADOW_MARGIN_DIP);
                }
            }
        }
    }
}

/// Inflate `rect` by `margin` DIP on every side and push it onto `out`, skipping
/// degenerate (non-positive area) rects so the region never gains an empty part.
#[inline]
fn push_inflated(
    out: &mut SmallVec<[bento_nano_style::Rect; 16]>,
    rect: bento_nano_style::Rect,
    margin: f32,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    out.push(bento_nano_style::Rect {
        x: rect.x - margin,
        y: rect.y - margin,
        width: rect.width + margin * 2.0,
        height: rect.height + margin * 2.0,
    });
}

/// Frosted-backdrop (2026-06-01) — straight per-channel colour lerp used by the
/// capsule↔panel morph to cross-fade `surface_zen → surface_expanded` along the
/// shared structural morph. `t` is clamped to
/// `[0, 1]`; every channel — including alpha — is interpolated linearly.
///
/// Deliberately a STRAIGHT lerp (not the premultiplied `Lerp for Color` in
/// `bento-nano-style`): both endpoints here are visible translucent surface
/// tints with similar hue, so the simple per-channel blend matches the CSS
/// `background` transition Tauri runs (which interpolates the rgba components
/// directly) and keeps the helper trivially testable. Free function so the
/// math is unit-tested without a GPU-backed `Renderer`.
#[inline]
fn vertical_gradient_props(rect: bento_nano_style::Rect) -> D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
        startPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        endPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.bottom(),
        },
    }
}

#[inline]
fn stack_capsule_sheen_gradient_props(
    rect: bento_nano_style::Rect,
) -> D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
        startPoint: D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        endPoint: D2D_POINT_2F {
            x: rect.right(),
            y: rect.bottom(),
        },
    }
}

#[inline]
fn d2d_gradient_stop(position: f32, color: Color) -> D2D1_GRADIENT_STOP {
    D2D1_GRADIENT_STOP {
        position: position.clamp(0.0, 1.0),
        color: D2D1_COLOR_F {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        },
    }
}

#[inline]
fn translate_rect(rect: bento_nano_style::Rect, dx: f32, dy: f32) -> bento_nano_style::Rect {
    bento_nano_style::Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        ..rect
    }
}

#[inline]
fn scale_rect_about_center(
    rect: bento_nano_style::Rect,
    center_rect: bento_nano_style::Rect,
    scale: f32,
) -> bento_nano_style::Rect {
    let scale = scale.max(0.0);
    let origin_x = center_rect.x + center_rect.width * 0.5;
    let origin_y = center_rect.y + center_rect.height * 0.5;
    let rect_cx = rect.x + rect.width * 0.5;
    let rect_cy = rect.y + rect.height * 0.5;
    let next_w = rect.width * scale;
    let next_h = rect.height * scale;
    let next_cx = origin_x + (rect_cx - origin_x) * scale;
    let next_cy = origin_y + (rect_cy - origin_y) * scale;
    bento_nano_style::Rect {
        x: next_cx - next_w * 0.5,
        y: next_cy - next_h * 0.5,
        width: next_w,
        height: next_h,
    }
}

#[inline]
fn scale_about_rect_center_matrix(
    base_scale: f32,
    center_rect: bento_nano_style::Rect,
    scale: f32,
) -> windows::Foundation::Numerics::Matrix3x2 {
    let scale = scale.max(0.0);
    let origin_x = center_rect.x + center_rect.width * 0.5;
    let origin_y = center_rect.y + center_rect.height * 0.5;
    let combined = base_scale * scale;
    windows::Foundation::Numerics::Matrix3x2 {
        M11: combined,
        M12: 0.0,
        M21: 0.0,
        M22: combined,
        M31: origin_x * (1.0 - scale) * base_scale,
        M32: origin_y * (1.0 - scale) * base_scale,
    }
}

#[inline]
fn stack_capsule_bloom_text_transform(
    base_scale: f32,
    center_rect: bento_nano_style::Rect,
    visual_scale: f32,
) -> Option<windows::Foundation::Numerics::Matrix3x2> {
    let visual_scale = visual_scale.max(0.0);
    if (visual_scale - 1.0).abs() <= f32::EPSILON {
        return None;
    }
    Some(scale_about_rect_center_matrix(
        base_scale,
        center_rect,
        visual_scale,
    ))
}

#[inline]
fn scale_border_radius(radius: BorderRadius, scale: f32) -> BorderRadius {
    let scale = scale.max(0.0);
    BorderRadius {
        top_left: radius.top_left * scale,
        top_right: radius.top_right * scale,
        bottom_right: radius.bottom_right * scale,
        bottom_left: radius.bottom_left * scale,
    }
}

#[inline]
fn scale_shadow(shadow: Shadow, scale: f32) -> Shadow {
    let scale = scale.max(0.0);
    Shadow {
        offset_x: shadow.offset_x * scale,
        offset_y: shadow.offset_y * scale,
        blur: shadow.blur * scale,
        spread: shadow.spread * scale,
        ..shadow
    }
}

#[inline]
fn scale_shadow_stack(stack: ShadowStack, scale: f32) -> ShadowStack {
    match stack.len() {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(scale_shadow(stack.inner(), scale)),
        _ => ShadowStack::two(
            scale_shadow(stack.inner(), scale),
            scale_shadow(stack.outer(), scale),
        ),
    }
}

#[inline]
fn fade_shadow(shadow: Shadow, opacity: f32) -> Shadow {
    Shadow {
        color: fade_color(shadow.color, opacity),
        ..shadow
    }
}

#[inline]
fn fade_shadow_stack(stack: ShadowStack, opacity: f32) -> ShadowStack {
    match stack.len() {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(fade_shadow(stack.inner(), opacity)),
        _ => ShadowStack::two(
            fade_shadow(stack.inner(), opacity),
            fade_shadow(stack.outer(), opacity),
        ),
    }
}

#[inline]
fn stack_capsule_hover_translate_y(hover_t: f32) -> f32 {
    -hover_t.clamp(0.0, 1.0)
}

#[inline]
fn stack_capsule_bloom_visual_for_app(
    app: &AppState,
    anchor_id: ZoneId,
    member_count: usize,
) -> StackCapsuleBloomVisual {
    let bloom_allowed = stack_surface_allows_bloom(app);
    if !bloom_allowed {
        return stack_capsule_bloom_visual(0.0, member_count, false);
    }
    let state_anchor = app.stack_bloom_anchor.get();
    if state_anchor != Some(anchor_id) {
        return stack_capsule_bloom_visual(0.0, member_count, false);
    }
    let leaving = app.stack_bloom_leaving.get();
    stack_capsule_bloom_visual(app.stack_bloom_progress.get(), member_count, leaving)
}

#[inline]
fn stack_capsule_bloom_visual(
    progress: f32,
    member_count: usize,
    leaving: bool,
) -> StackCapsuleBloomVisual {
    let progress = progress.clamp(0.0, 1.0);
    let recede_t = if leaving {
        1.0 - zone_pill_geometry::ease_out_back_progress(progress).clamp(0.0, 1.0)
    } else {
        let reveal_ms = stack_tray::stack_bloom_reveal_duration_ms(member_count) as f32;
        let local = (progress * reveal_ms / STACK_CAPSULE_BLOOMED_RECEDES_MS).clamp(0.0, 1.0);
        zone_pill_geometry::ease_out_back_progress(local).clamp(0.0, 1.0)
    };
    StackCapsuleBloomVisual {
        recede_t,
        scale: 1.0 + (STACK_CAPSULE_BLOOMED_SCALE - 1.0) * recede_t,
        opacity: 1.0 + (STACK_CAPSULE_BLOOMED_OPACITY - 1.0) * recede_t,
    }
}

/// Tauri `spring-emerge`: 0% scale(.96)/opacity(0), 60%
/// scale(1.02)/opacity(1), 100% scale(1)/opacity(1), with the same spring
/// bezier applied to each keyframe interval.
#[inline]
fn stack_capsule_emerge_visual(progress: f32) -> StackCapsuleEmergeVisual {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= STACK_CAPSULE_EMERGE_OVERSHOOT_AT {
        let local = progress / STACK_CAPSULE_EMERGE_OVERSHOOT_AT;
        let eased = zone_pill_geometry::ease_out_back_progress(local);
        StackCapsuleEmergeVisual {
            scale: STACK_CAPSULE_EMERGE_START_SCALE
                + (STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE - STACK_CAPSULE_EMERGE_START_SCALE) * eased,
            opacity: eased.clamp(0.0, 1.0),
        }
    } else {
        let local = (progress - STACK_CAPSULE_EMERGE_OVERSHOOT_AT)
            / (1.0 - STACK_CAPSULE_EMERGE_OVERSHOOT_AT);
        let eased = zone_pill_geometry::ease_out_back_progress(local);
        StackCapsuleEmergeVisual {
            scale: STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE
                + (1.0 - STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE) * eased,
            opacity: 1.0,
        }
    }
}

#[inline]
fn stack_capsule_presented_emerge_visual(progress: f32) -> StackCapsuleEmergeVisual {
    stack_capsule_emerge_visual(progress.max(STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS))
}

#[inline]
fn stack_capsule_bloomed_target_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 14.0, 36.0, Color::rgba(0.0, 0.0, 0.0, 0.22)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::rgba(1.0, 1.0, 1.0, 0.06),
        },
    )
}

#[inline]
fn stack_capsule_bloom_shadow_stack(idle: ShadowStack, hover_t: f32, recede_t: f32) -> ShadowStack {
    let base = stack_capsule_hover_shadow_stack(idle, hover_t);
    let t = recede_t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return base;
    }
    let target = stack_capsule_bloomed_target_shadow_stack();
    let base_outer = if base.len() >= 2 {
        base.outer()
    } else {
        Shadow::NONE
    };
    ShadowStack::two(
        lerp_shadow(base.inner(), target.inner(), t),
        lerp_shadow(base_outer, target.outer(), t),
    )
}

#[inline]
fn stack_capsule_bloom_border_color(
    pal: bento_nano_style::tokens::PaletteTauri,
    hover_t: f32,
    recede_t: f32,
) -> Color {
    lerp_color(
        stack_capsule_hover_border_color(pal, hover_t),
        Color::rgba(1.0, 1.0, 1.0, 0.18),
        recede_t.clamp(0.0, 1.0),
    )
}

#[inline]
fn stack_capsule_hover_target_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 18.0, 42.0, Color::rgba(0.0, 0.0, 0.0, 0.24)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::rgba(1.0, 1.0, 1.0, 0.04),
        },
    )
}

#[inline]
fn stack_capsule_preview_shadow_stack() -> ShadowStack {
    ShadowStack::two(
        Shadow::drop(0.0, 18.0, 42.0, Color::rgba(0.0, 0.0, 0.0, 0.24)),
        Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: STACK_CAPSULE_PREVIEW_RING,
        },
    )
}

#[inline]
fn stack_capsule_visual_shadow_stack(
    idle: ShadowStack,
    hover_t: f32,
    recede_t: f32,
    has_preview: bool,
) -> ShadowStack {
    if has_preview {
        return stack_capsule_preview_shadow_stack();
    }
    stack_capsule_bloom_shadow_stack(idle, hover_t, recede_t)
}

#[inline]
fn lerp_shadow(a: Shadow, b: Shadow, t: f32) -> Shadow {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Shadow {
        offset_x: a.offset_x * inv + b.offset_x * t,
        offset_y: a.offset_y * inv + b.offset_y * t,
        blur: a.blur * inv + b.blur * t,
        spread: a.spread * inv + b.spread * t,
        color: lerp_color(a.color, b.color, t),
    }
}

#[inline]
fn lerp_shadow_stack(a: ShadowStack, b: ShadowStack, t: f32) -> ShadowStack {
    let len = a.len().max(b.len());
    let layer = |stack: ShadowStack, index: usize| {
        stack.layers().get(index).copied().unwrap_or(Shadow::NONE)
    };
    match len {
        0 => ShadowStack::NONE,
        1 => ShadowStack::one(lerp_shadow(layer(a, 0), layer(b, 0), t)),
        _ => ShadowStack::two(
            lerp_shadow(layer(a, 0), layer(b, 0), t),
            lerp_shadow(layer(a, 1), layer(b, 1), t),
        ),
    }
}

#[inline]
fn stack_capsule_hover_shadow_stack(idle: ShadowStack, hover_t: f32) -> ShadowStack {
    let t = hover_t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return idle;
    }
    let hover = stack_capsule_hover_target_shadow_stack();
    let idle_outer = if idle.len() >= 2 {
        idle.outer()
    } else {
        Shadow::NONE
    };
    ShadowStack::two(
        lerp_shadow(idle.inner(), hover.inner(), t),
        lerp_shadow(idle_outer, hover.outer(), t),
    )
}

#[inline]
fn stack_capsule_hover_border_color(
    pal: bento_nano_style::tokens::PaletteTauri,
    hover_t: f32,
) -> Color {
    lerp_color(
        pal.border_zen,
        Color::rgba(1.0, 1.0, 1.0, 0.18),
        hover_t.clamp(0.0, 1.0),
    )
}

#[inline]
fn collapsed_zen_surface_color(
    pal: bento_nano_style::tokens::PaletteTauri,
    _hover_t: f32,
) -> Color {
    pal.surface_zen
}

const ORDINARY_MEDIUM_PILL_SHADOW_OPACITY: f32 = 0.30;
const ORDINARY_LARGE_PILL_SHADOW_OPACITY: f32 = 0.22;

#[inline]
fn ordinary_zone_pill_shadow_stack(
    size: crate::business::zen_capsule::CapsuleSize,
    stack: ShadowStack,
) -> ShadowStack {
    match size {
        crate::business::zen_capsule::CapsuleSize::Medium => {
            fade_shadow_stack(stack, ORDINARY_MEDIUM_PILL_SHADOW_OPACITY)
        }
        crate::business::zen_capsule::CapsuleSize::Large => {
            fade_shadow_stack(stack, ORDINARY_LARGE_PILL_SHADOW_OPACITY)
        }
        crate::business::zen_capsule::CapsuleSize::Small => stack,
    }
}

#[inline]
fn ordinary_zone_pill_chrome_radius(rect: Rect, radius: BorderRadius) -> BorderRadius {
    let max_radius = rect.height * 0.5;
    BorderRadius {
        top_left: radius.top_left.min(max_radius),
        top_right: radius.top_right.min(max_radius),
        bottom_right: radius.bottom_right.min(max_radius),
        bottom_left: radius.bottom_left.min(max_radius),
    }
}

#[inline]
fn frosted_backdrop_saturation_for_palette(pal: bento_nano_style::tokens::PaletteTauri) -> f32 {
    if pal.is_dark {
        FROSTED_BACKDROP_SATURATION_DARK
    } else {
        FROSTED_BACKDROP_SATURATION_LIGHT
    }
}

#[inline]
fn frosted_backdrop_saturation_changed(cached: f32, desired: f32) -> bool {
    (cached - desired).abs() > f32::EPSILON
}

/// Main is desktop-embedded, so a palette saturation change can safely refresh
/// its desktop snapshot. Settings is a full-work-area modal: recapturing while
/// it is visible would photograph its own scrim and recursively darken the
/// panel after every theme switch. It therefore reuses the clean snapshot from
/// open and captures the new saturation on the next reopen.
#[inline]
fn frosted_backdrop_saturation_recapture_needed(
    kind: WindowKind,
    cached: f32,
    desired: f32,
) -> bool {
    kind == WindowKind::Main && frosted_backdrop_saturation_changed(cached, desired)
}

#[inline]
fn stack_capsule_glass_sheen_colors() -> (Color, Color) {
    (
        with_alpha(Color::WHITE, 0.08),
        with_alpha(Color::WHITE, 0.02),
    )
}

#[inline]
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color {
        r: a.r * inv + b.r * t,
        g: a.g * inv + b.g * t,
        b: a.b * inv + b.b * t,
        a: a.a * inv + b.a * t,
    }
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
    // Z-order (2026-06-02) — mirror the two-layer draw stack: test `on_top`
    // (expanded/morphing) zones BEFORE `!on_top` (pills) so a point inside an
    // expanded panel resolves to the panel, never a pill drawn behind it. Within
    // each layer keep the existing reverse/topmost order. Uses the shared
    // `AppState::zone_on_top` SSoT so this drag-drop targeting can't drift from
    // the painted stack. (Drop targeting keys off the full stored zone rect.)
    for on_top_layer in [true, false] {
        for zone in app.zones.iter().rev() {
            if !zone.is_visible() || zone.is_stacked_child() {
                continue;
            }
            if app.zone_on_top(zone) != on_top_layer {
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
    }
    None
}

fn drop_preview_rect_for_zone(
    zone: &Zone,
    drag: Option<ActiveItemDragVisual>,
    is_wide: bool,
    scroll_offset: f32,
    item_top_offset: f32,
) -> Option<bento_nano_style::Rect> {
    let drag = drag?;
    let (grid_x, grid_y) = item_grid_position_for_zone(
        zone,
        drag.last_x,
        drag.last_y,
        scroll_offset,
        item_top_offset,
    );
    let mut rect = item_card_rect_for_grid(zone, grid_x, grid_y, is_wide);
    rect.y += item_top_offset - scroll_offset;
    rect.height = item_grid::ITEM_GRID_ROW_HEIGHT_PX;
    (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
}

fn item_grid_position_for_zone(
    zone: &Zone,
    x: f32,
    y: f32,
    scroll_offset: f32,
    item_top_offset: f32,
) -> (i32, i32) {
    let gap = item_grid::ITEM_GRID_COLUMN_GAP_PX;
    // P3.5 (1:1) — mirror the paint-side horizontal grid inset (`HEADER_INSET_X`
    // = 16 per side) so the drag-position hit math stays in lockstep with the
    // painted card rects (`highlight_overlay::item_card_rect_for_grid`).
    let inset_x = expanded_zone_grid::HEADER_INSET_X;
    let columns =
        item_grid::effective_column_count(zone.w as f32, zone.grid_columns.max(1), inset_x).max(1)
            as i32;
    let columns_f = columns as f32;
    let cell_w = ((zone.w as f32 - inset_x * 2.0) - gap * (columns_f - 1.0)).max(44.0) / columns_f;
    let col_stride = cell_w + gap;
    let row_stride = item_grid::ITEM_GRID_ROW_HEIGHT_PX + item_grid::ITEM_GRID_ROW_GAP_PX;
    let raw_col = ((x - zone.x as f32 - inset_x) / col_stride).floor() as i32;
    let raw_row =
        ((y + scroll_offset - zone.y as f32 - item_grid::ITEM_GRID_TOP_OFFSET_PX - item_top_offset)
            / row_stride)
            .floor() as i32;
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

fn item_card_rect_for_item(zone: &Zone, item: &ZoneItem) -> bento_nano_style::Rect {
    highlight_overlay::item_card_rect_for_item(zone, item)
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

fn centered_square_rect(rect: bento_nano_style::Rect, size: f32) -> bento_nano_style::Rect {
    let size = size.max(0.0).min(rect.width).min(rect.height);
    bento_nano_style::Rect {
        x: rect.x + (rect.width - size) * 0.5,
        y: rect.y + (rect.height - size) * 0.5,
        width: size,
        height: size,
    }
}

#[inline]
fn stack_bloom_active_transition_t(now_ms: u32, started_ms: u32) -> f32 {
    let raw = now_ms.wrapping_sub(started_ms) as f32 / STACK_BLOOM_ACTIVE_TRANSITION_MS as f32;
    animator::ease_in_out_quad(raw.clamp(0.0, 1.0))
}

/// Return the active petal's crisp outer-halo spread and alpha.
///
/// This deliberately models only the CSS spread rings. Reintroducing the
/// reference's blurred black elevation layers would recreate R13-01's broad
/// dark cloud in the native renderer.
#[inline]
fn stack_bloom_active_pulse(now_ms: u32, started_ms: u32, many_members: bool) -> (f32, f32) {
    if many_members {
        return (4.0, 0.18);
    }
    let elapsed = now_ms.wrapping_sub(started_ms);
    if elapsed <= STACK_BLOOM_ACTIVE_PULSE_DELAY_MS {
        return (5.5, 0.16);
    }
    let phase = (elapsed - STACK_BLOOM_ACTIVE_PULSE_DELAY_MS) % STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS;
    let phase = phase as f32 / STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS as f32;
    let triangle = if phase <= 0.5 {
        phase * 2.0
    } else {
        (1.0 - phase) * 2.0
    };
    let t = animator::ease_in_out_quad(triangle);
    (5.5 + 1.5 * t, 0.16 + 0.06 * t)
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

/// W13-B — retain only zero-blur outline/ring geometry from a shadow token.
/// Blurred layers return `None`; drawing them as larger solid fills creates a
/// dark halo rather than a Gaussian shadow.
fn crisp_shadow_rect(
    base: bento_nano_style::Rect,
    layer: bento_nano_style::Shadow,
) -> Option<bento_nano_style::Rect> {
    if layer.color.a <= 0.0 || layer.blur > 0.5 {
        return None;
    }
    let grow = layer.spread.max(0.0);
    Some(bento_nano_style::Rect {
        x: base.x + layer.offset_x - grow,
        y: base.y + layer.offset_y - grow,
        width: base.width + grow * 2.0,
        height: base.height + grow * 2.0,
    })
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

fn grid_columns_label(columns: u32, zh: bool) -> &'static str {
    match (columns, zh) {
        (2, true) => "2 列",
        (3, true) => "3 列",
        (4, true) => "4 列",
        (5, true) => "5 列",
        (6, true) => "6 列",
        (_, true) => "4 列",
        (2, false) => "2 columns",
        (3, false) => "3 columns",
        (4, false) => "4 columns",
        (5, false) => "5 columns",
        (6, false) => "6 columns",
        (_, false) => "4 columns",
    }
}

/// Wave C — format a zone item count for the collapsed pill badge. Caps
/// the display at "99+" so the badge geometry doesn't need to grow past
/// `PILL_BADGE_MIN_WIDTH` for typical zones; >999 items is still rendered
/// as "999+" so the result fits the 4-digit budget in
/// `zone_pill_geometry::badge_width_for_count`.
/// Floor retained only for the legacy stack-capsule title shrink path. Ordinary
/// Zone pills use a fixed readable role plus DWrite ellipsis.
const PILL_TITLE_MIN_FONT_PX: f32 = 8.0;

/// G5 (2026-06-01) — quantised cache signature for the stack title shrink memo.
/// Folds the label bytes, the available width (rounded to whole DIPs) and the
/// tier base font (×4, rounded), weight, and tracking into one `u64`. A
/// per-frame re-paint of the SAME label at the SAME width/typography hashes
/// identically → cache hit → no DWrite measure, no allocation (§10). Collisions
/// only over-trigger a (correct) re-measure, never a wrong size.
fn title_shrink_signature(
    label: &str,
    avail_w: f32,
    base_px: f32,
    weight: u16,
    tracking: f32,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    label.hash(&mut h);
    (avail_w.max(0.0).round() as u32).hash(&mut h);
    ((base_px.max(0.0) * 4.0).round() as u32).hash(&mut h);
    weight.hash(&mut h);
    ((tracking.max(0.0) * 100.0).round() as u32).hash(&mut h);
    h.finish()
}

#[inline]
fn text_width_with_tracking(base_width: f32, utf16_units: usize, tracking: f32) -> f32 {
    let tracked_gaps = utf16_units.saturating_sub(1) as f32;
    (base_width + tracking * tracked_gaps).max(0.0)
}

/// G5 (2026-06-01) — pure font-shrink stepper for the pill title (`useTextAbbr`
/// parity), factored out for unit testing without DWrite. Walks the font size
/// down from `base_px` in 1px steps to `PILL_TITLE_MIN_FONT_PX`, calling
/// `measure(size)` (the rendered text width at that size) at each step, and
/// returns the FIRST (largest) size whose measured width fits `avail_w`. If
/// nothing fits down to the floor, returns the floor; callers still draw the
/// complete text, exactly as Tauri v7 `textAbbr.ts` does at `MIN_FONT_SIZE_PX`.
///
/// `measure` is assumed monotonic in size (smaller font ⇒ narrower run), so the
/// first fit found while stepping down is the largest fitting size.
fn shrink_font_to_fit(base_px: f32, avail_w: f32, mut measure: impl FnMut(f32) -> f32) -> f32 {
    let base = base_px.max(PILL_TITLE_MIN_FONT_PX);
    let mut size = base;
    // Step down in whole-px increments; the loop is bounded by the base→floor
    // span (≤ ~8 iterations for the 11/14/16px tiers), so it is cheap and total.
    while size >= PILL_TITLE_MIN_FONT_PX {
        if measure(size) <= avail_w {
            return size;
        }
        size -= 1.0;
    }
    PILL_TITLE_MIN_FONT_PX
}

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

#[inline]
fn fade_color(color: Color, opacity: f32) -> Color {
    with_alpha(color, color.a * opacity.clamp(0.0, 1.0))
}

#[inline]
fn settings_encryption_mode_button_fill_color(
    is_active: bool,
    is_hovered: bool,
    base_fill: Color,
    hover_fill: Color,
    active_fill: Color,
) -> Color {
    if is_active {
        active_fill
    } else if is_hovered {
        hover_fill
    } else {
        base_fill
    }
}

/// P1 (#7 fix wave 2026-06-01) — pure caret-blink phase. Given a wall-clock
/// `now_ms` (e.g. `GetTickCount`), returns whether the text-field caret should
/// be PAINTED this frame. Windows blinks the caret at ≈530ms (the default
/// `GetCaretBlinkTime`): ON for one 530ms half-period, OFF for the next. Pure
/// (no state, no allocation) so it's unit-testable and §10-safe.
pub fn settings_caret_on(now_ms: u32) -> bool {
    (now_ms / 530) % 2 == 0
}

/// P2 (#7 fix wave 2026-06-01) — the user-visible mode label that MATCHES the
/// mode-button TITLES (Tauri uses one `modeLabel()` for the current-mode value,
/// the buttons, AND the applied banner). Passphrase maps to the FULL token
/// (`ENCRYPTION_MODE_PASSPHRASE_FULL`, id 236 = 自定义口令), NOT the short id 86
/// (密码, `ENCRYPTION_MODE_PASSPHRASE`) the prior current-mode paint used.
/// None/DPAPI reuse the shared button ids — so the current-mode VALUE, the
/// active button TITLE, and the P9 applied banner all read identically (the
/// parity invariant). Replaces the old `localized_encryption_mode` short-label
/// helper, which had no remaining call site after this fix.
fn localized_encryption_mode_button_label(
    mode: crate::state::SettingsEncryptionMode,
) -> &'static str {
    use crate::state::SettingsEncryptionMode;
    use bento_nano_style::i18n_zh_cn::ids;
    match mode {
        SettingsEncryptionMode::None => bento_nano_style::t(ids::ENCRYPTION_MODE_NONE),
        SettingsEncryptionMode::Dpapi => bento_nano_style::t(ids::ENCRYPTION_MODE_DPAPI),
        SettingsEncryptionMode::Passphrase => {
            bento_nano_style::t(ids::ENCRYPTION_MODE_PASSPHRASE_FULL)
        }
    }
}

/// G3 — locale-aware version of `SettingsUpdaterStatus::summary()`. The static
/// `Idle` / `Checking` tokens are translated; the version-bearing variants
/// keep the existing `format!` shape (with a localized prefix) so the
/// "Available 2.1.0" / "Downloading 4096/8192 B" inline-test expectations in
/// `bento-nano-shell` still hold for the en-US locale.
//
// β carry-over (Wave I-α / R14 2026-05-25): Wave H baseline leftover. Updater
// summary row not yet wired into the Settings panel; β1 owner of updater UI
// will either delete or call.
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

fn icon_kind_label(kind: IconKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (IconKind::Folder, true) => "文件夹",
        (IconKind::Document, true) => "文档",
        (IconKind::Image, true) => "图片",
        (IconKind::Music, true) => "音乐",
        (IconKind::Video, true) => "视频",
        (IconKind::Code, true) => "代码",
        (IconKind::Download, true) => "下载",
        (IconKind::Archive, true) => "归档",
        (IconKind::Star, true) => "收藏",
        (IconKind::Bookmark, true) => "书签",
        (IconKind::Tag, true) => "标签",
        (IconKind::Globe, true) => "网络",
        (IconKind::Lightning, true) => "快捷",
        (IconKind::Briefcase, true) => "工作",
        (IconKind::Gamepad, true) => "游戏",
        (IconKind::Palette, true) => "调色板",
        (IconKind::ArrowRight, true) => "箭头",
        (IconKind::Trash, true) => "回收站",
        (IconKind::Search, true) => "搜索",
        (IconKind::Copy, true) => "复制",
        (IconKind::ExternalLink, true) => "外部链接",
        (IconKind::FolderOpen, true) => "打开文件夹",
        (IconKind::Camera, true) => "相机",
        (IconKind::Columns, true) => "分栏",
        (IconKind::X, true) => "关闭",
        (IconKind::Edit, true) => "编辑",
        (IconKind::Grid, true) => "网格",
        (IconKind::Square, true) => "方框",
        (IconKind::Pin, true) => "固定",
        (IconKind::Settings, true) => "设置",
        (IconKind::Folder, false) => "Folder",
        (IconKind::Document, false) => "Document",
        (IconKind::Image, false) => "Image",
        (IconKind::Music, false) => "Music",
        (IconKind::Video, false) => "Video",
        (IconKind::Code, false) => "Code",
        (IconKind::Download, false) => "Download",
        (IconKind::Archive, false) => "Archive",
        (IconKind::Star, false) => "Star",
        (IconKind::Bookmark, false) => "Bookmark",
        (IconKind::Tag, false) => "Tag",
        (IconKind::Globe, false) => "Globe",
        (IconKind::Lightning, false) => "Lightning",
        (IconKind::Briefcase, false) => "Briefcase",
        (IconKind::Gamepad, false) => "Gamepad",
        (IconKind::Palette, false) => "Palette",
        (IconKind::ArrowRight, false) => "Arrow",
        (IconKind::Trash, false) => "Trash",
        (IconKind::Search, false) => "Search",
        (IconKind::Copy, false) => "Copy",
        (IconKind::ExternalLink, false) => "External link",
        (IconKind::FolderOpen, false) => "Open folder",
        (IconKind::Camera, false) => "Camera",
        (IconKind::Columns, false) => "Columns",
        (IconKind::X, false) => "Close",
        (IconKind::Edit, false) => "Edit",
        (IconKind::Grid, false) => "Grid",
        (IconKind::Square, false) => "Square",
        (IconKind::Pin, false) => "Pin",
        (IconKind::Settings, false) => "Settings",
    }
}

fn localized_icon_wire_label(wire: &str, zh: bool) -> &str {
    IconKind::from_str_opt(wire)
        .map(|kind| icon_kind_label(kind, zh))
        .unwrap_or(wire)
}

fn localized_visible_range(
    start: usize,
    count: usize,
    visible_limit: usize,
    zh: bool,
) -> Option<SmolStr> {
    if count <= visible_limit {
        return None;
    }
    let start = start.min(count.saturating_sub(visible_limit));
    let end = count.min(start + visible_limit);
    Some(SmolStr::new(if zh {
        format!("第 {}–{} 项，共 {} 项", start + 1, end, count)
    } else {
        format!("Items {}–{} of {}", start + 1, end, count)
    }))
}

fn bulk_manager_action_label(
    hit: bulk_manager_panel::BulkManagerPointerHit,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkManagerPointerHit as Hit;
    match (hit, zh) {
        (Hit::SelectAll, true) => "全选",
        (Hit::Invert, true) => "反选",
        (Hit::Hide, true) => "隐藏",
        (Hit::Show, true) => "显示",
        (Hit::LayoutGrid, true) => "网格",
        (Hit::LayoutRow, true) => "横排",
        (Hit::LayoutColumn, true) => "纵列",
        (Hit::LayoutSpiral, true) => "环绕",
        (Hit::LayoutOrganic, true) => "自然",
        (Hit::Update, true) => "刷新",
        (Hit::TextEdit, true) => "文字",
        (Hit::IconPicker, true) => "图标",
        (Hit::AccentPicker, true) => "颜色",
        (Hit::Delete, true) => "删除",
        (Hit::Move, true) => "移动",
        (Hit::Close, true) => "关闭",
        (Hit::SelectAll, false) => "All",
        (Hit::Invert, false) => "Invert",
        (Hit::Hide, false) => "Hide",
        (Hit::Show, false) => "Show",
        (Hit::LayoutGrid, false) => "Grid",
        (Hit::LayoutRow, false) => "Row",
        (Hit::LayoutColumn, false) => "Column",
        (Hit::LayoutSpiral, false) => "Spiral",
        (Hit::LayoutOrganic, false) => "Organic",
        (Hit::Update, false) => "Refresh",
        (Hit::TextEdit, false) => "Text",
        (Hit::IconPicker, false) => "Icon",
        (Hit::AccentPicker, false) => "Color",
        (Hit::Delete, false) => "Delete",
        (Hit::Move, false) => "Move",
        (Hit::Close, false) => "Close",
        (Hit::SearchInput | Hit::Sort(_) | Hit::Row(_), _) => "",
    }
}

fn bulk_manager_sort_label(key: bulk_manager_panel::SortKey, zh: bool) -> &'static str {
    use bulk_manager_panel::SortKey;
    match (key, zh) {
        (SortKey::Name, true) => "名称",
        (SortKey::Items, true) => "项目数",
        (SortKey::Accent, true) => "颜色",
        (SortKey::Size, true) => "尺寸",
        (SortKey::Name, false) => "Name",
        (SortKey::Items, false) => "Items",
        (SortKey::Accent, false) => "Accent",
        (SortKey::Size, false) => "Size",
    }
}

fn bulk_text_edit_field_label(
    field: bulk_manager_panel::BulkTextEditField,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkTextEditField;
    match (field, zh) {
        (BulkTextEditField::Alias, true) => "别名",
        (BulkTextEditField::Icon, true) => "图标",
        (BulkTextEditField::Accent, true) => "颜色",
        (BulkTextEditField::CapsuleSize, true) => "胶囊尺寸",
        (BulkTextEditField::DisplayMode, true) => "显示模式",
        (BulkTextEditField::Alias, false) => "alias",
        (BulkTextEditField::Icon, false) => "icon",
        (BulkTextEditField::Accent, false) => "accent",
        (BulkTextEditField::CapsuleSize, false) => "capsule size",
        (BulkTextEditField::DisplayMode, false) => "display mode",
    }
}

fn bulk_text_edit_placeholder(
    field: bulk_manager_panel::BulkTextEditField,
    zh: bool,
) -> &'static str {
    use bulk_manager_panel::BulkTextEditField;
    match (field, zh) {
        (BulkTextEditField::Alias, true) => "留空可清除别名",
        (BulkTextEditField::Icon, true) => "例如 folder、star、archive",
        (BulkTextEditField::Accent, true) => "例如 #3b82f6",
        (BulkTextEditField::CapsuleSize, true) => "small / medium / large",
        (BulkTextEditField::DisplayMode, true) => "hover / always / click / clear",
        (_, false) => field.placeholder(),
    }
}

fn timeline_action_label(hit: timeline_panel::TimelinePointerHit, zh: bool) -> &'static str {
    use timeline_panel::TimelinePointerHit as Hit;
    match (hit, zh) {
        (Hit::Save, true) => "保存",
        (Hit::Pin, true) => "固定",
        (Hit::Restore, true) => "恢复",
        (Hit::Delete, true) => "删除",
        (Hit::Close, true) => "关闭",
        (Hit::Save, false) => "Save",
        (Hit::Pin, false) => "Pin",
        (Hit::Restore, false) => "Restore",
        (Hit::Delete, false) => "Delete",
        (Hit::Close, false) => "Close",
        (Hit::Row(_), _) => "",
    }
}

fn capsule_action_label(hit: CapsulePickerHit, zh: bool) -> &'static str {
    match (hit, zh) {
        (CapsulePickerHit::Capture, true) => "保存当前",
        (CapsulePickerHit::Restore, true) => "恢复",
        (CapsulePickerHit::Delete, true) => "删除",
        (CapsulePickerHit::Close, true) => "关闭",
        (CapsulePickerHit::Capture, false) => "Save current",
        (CapsulePickerHit::Restore, false) => "Restore",
        (CapsulePickerHit::Delete, false) => "Delete",
        (CapsulePickerHit::Close, false) => "Close",
        (
            CapsulePickerHit::Hint
            | CapsulePickerHit::Error
            | CapsulePickerHit::Empty
            | CapsulePickerHit::Row(_),
            _,
        ) => "",
    }
}

fn snapshot_action_label(hit: snapshot_picker::SnapshotPickerPointerHit, zh: bool) -> &'static str {
    use snapshot_picker::SnapshotPickerPointerHit as Hit;
    match (hit, zh) {
        (Hit::Save, true) => "保存",
        (Hit::Load, true) => "载入",
        (Hit::Delete, true) => "删除",
        (Hit::Timeline, true) => "时间线",
        (Hit::Close, true) => "关闭",
        (Hit::Save, false) => "Save",
        (Hit::Load, false) => "Load",
        (Hit::Delete, false) => "Delete",
        (Hit::Timeline, false) => "Timeline",
        (Hit::Close, false) => "Close",
        (Hit::Row(_), _) => "",
    }
}

fn rules_action_label(
    hit: rules_wizard::RulesWizardPointerHit,
    step: WizardStep,
    zh: bool,
) -> &'static str {
    use rules_wizard::RulesWizardPointerHit as Hit;
    match (hit, step, zh) {
        (Hit::NextSave, WizardStep::Review, true) => "保存",
        (Hit::NextSave, _, true) => "下一步",
        (Hit::Predicate, _, true) => "条件",
        (Hit::Action, _, true) => "操作",
        (Hit::RunMode, _, true) => "运行",
        (Hit::Combine, _, true) => "关系",
        (Hit::AddCondition, _, true) => "添加",
        (Hit::RemoveCondition, _, true) => "移除",
        (Hit::NextCondition, _, true) => "下一项",
        (Hit::Edit, _, true) => "编辑",
        (Hit::Run, _, true) => "运行",
        (Hit::Delete, _, true) => "删除",
        (Hit::Close, _, true) => "关闭",
        (Hit::NextSave, WizardStep::Review, false) => "Save",
        (Hit::NextSave, _, false) => "Next",
        (Hit::Predicate, _, false) => "When",
        (Hit::Action, _, false) => "Action",
        (Hit::RunMode, _, false) => "Run",
        (Hit::Combine, _, false) => "All/Any",
        (Hit::AddCondition, _, false) => "Add",
        (Hit::RemoveCondition, _, false) => "Remove",
        (Hit::NextCondition, _, false) => "Next",
        (Hit::Edit, _, false) => "Edit",
        (Hit::Run, _, false) => "Run",
        (Hit::Delete, _, false) => "Delete",
        (Hit::Close, _, false) => "Close",
        (Hit::ConditionRow(_) | Hit::Row(_), _, _) => "",
    }
}

fn wizard_step_label(step: WizardStep, zh: bool) -> &'static str {
    match (step, zh) {
        (WizardStep::Conditions, true) => "条件",
        (WizardStep::Action, true) => "操作",
        (WizardStep::Preview, true) => "预览",
        (WizardStep::Name, true) => "命名",
        (WizardStep::Review, true) => "确认",
        (WizardStep::Conditions, false) => "Conditions",
        (WizardStep::Action, false) => "Action",
        (WizardStep::Preview, false) => "Preview",
        (WizardStep::Name, false) => "Name",
        (WizardStep::Review, false) => "Review",
    }
}

fn combine_label(mode: rules_wizard::CombineMode, zh: bool) -> &'static str {
    match (mode, zh) {
        (rules_wizard::CombineMode::All, true) => "全部满足",
        (rules_wizard::CombineMode::Any, true) => "任一满足",
        (rules_wizard::CombineMode::All, false) => "all",
        (rules_wizard::CombineMode::Any, false) => "any",
    }
}

fn predicate_label(kind: PredicateKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (PredicateKind::NameStartsWith, true) => "名称开头是",
        (PredicateKind::NameContains, true) => "名称包含",
        (PredicateKind::NameEndsWith, true) => "名称结尾是",
        (PredicateKind::ExtensionIn, true) => "扩展名属于",
        (PredicateKind::CreatedBefore, true) => "创建时间早于指定天数",
        (PredicateKind::ModifiedBefore, true) => "修改时间早于指定天数",
        (PredicateKind::SizeGreaterThan, true) => "文件大于指定大小",
        (PredicateKind::InZone, true) => "位于区域",
        (PredicateKind::OnDesktop, true) => "位于桌面",
        (PredicateKind::NameStartsWith, false) => "name starts with",
        (PredicateKind::NameContains, false) => "name contains",
        (PredicateKind::NameEndsWith, false) => "name ends with",
        (PredicateKind::ExtensionIn, false) => "extension in",
        (PredicateKind::CreatedBefore, false) => "created before days",
        (PredicateKind::ModifiedBefore, false) => "modified before days",
        (PredicateKind::SizeGreaterThan, false) => "size greater than",
        (PredicateKind::InZone, false) => "in zone",
        (PredicateKind::OnDesktop, false) => "on desktop",
    }
}

fn action_label(kind: ActionKind, zh: bool) -> &'static str {
    match (kind, zh) {
        (ActionKind::MoveToZone, true) => "移动到区域",
        (ActionKind::MoveToFolder, true) => "移动到文件夹",
        (ActionKind::DeleteToRecycleBin, true) => "移入回收站",
        (ActionKind::Tag, true) => "添加标签",
        (ActionKind::Notify, true) => "发送通知",
        (ActionKind::MoveToZone, false) => "move to zone",
        (ActionKind::MoveToFolder, false) => "move to folder",
        (ActionKind::DeleteToRecycleBin, false) => "delete to recycle bin",
        (ActionKind::Tag, false) => "tag",
        (ActionKind::Notify, false) => "notify",
    }
}

fn run_mode_label(mode: RunModeChoice, zh: bool) -> &'static str {
    match (mode, zh) {
        (RunModeChoice::OnDemand, true) => "手动运行",
        (RunModeChoice::OnFileChange, true) => "文件变化时运行",
        (RunModeChoice::Interval, true) => "定时运行",
        (RunModeChoice::OnDemand, false) => "on demand",
        (RunModeChoice::OnFileChange, false) => "on file change",
        (RunModeChoice::Interval, false) => "interval",
    }
}

fn rules_preview_hit_label(hit: &str, index: usize, zh: bool) -> SmolStr {
    SmolStr::new(if zh {
        format!("命中项 {}：{hit}", index + 1)
    } else {
        format!("Match {}: {hit}", index + 1)
    })
}

fn confidence_tone_label(tone: smart_group_suggestor::ConfidenceTone, zh: bool) -> &'static str {
    use smart_group_suggestor::ConfidenceTone;
    match (tone, zh) {
        (ConfidenceTone::Low, true) => "低",
        (ConfidenceTone::Medium, true) => "中",
        (ConfidenceTone::High, true) => "高",
        (ConfidenceTone::Low, false) => "Low",
        (ConfidenceTone::Medium, false) => "Medium",
        (ConfidenceTone::High, false) => "High",
    }
}

fn localized_suggestor_group_name(name: &str, zh: bool) -> &str {
    if !zh {
        return name;
    }
    match name {
        "Documents" => "文档",
        "Images" => "图片",
        "Videos" => "视频",
        "Audio" => "音频",
        "Code" => "代码",
        "Archives" => "压缩包",
        "Executables" => "程序",
        "Shortcuts" => "快捷方式",
        "Today" => "今天",
        "This Week" => "本周",
        "This Month" => "本月",
        "Older" => "更早",
        _ => name,
    }
}

fn localized_suggestor_rule_summary(
    suggestion: &bento_nano_backend::grouping::SuggestedGroup,
    zh: bool,
) -> SmolStr {
    use bento_nano_backend::layout::GroupRuleType;
    match suggestion.rule.rule_type {
        GroupRuleType::Extension => suggestion
            .rule
            .extensions
            .as_ref()
            .filter(|extensions| !extensions.is_empty())
            .map(|extensions| SmolStr::new(extensions.join(", ")))
            .unwrap_or_else(|| SmolStr::new_static(if zh { "按扩展名" } else { "Extension" })),
        GroupRuleType::NamePattern => suggestion
            .rule
            .pattern
            .as_deref()
            .filter(|pattern| !pattern.trim().is_empty())
            .map(SmolStr::new)
            .unwrap_or_else(|| {
                SmolStr::new_static(if zh {
                    "按名称模式"
                } else {
                    "Name pattern"
                })
            }),
        GroupRuleType::ModifiedDate => SmolStr::new_static(if zh {
            "按修改时间"
        } else {
            "Modified date"
        }),
    }
}

#[cfg(test)]
mod auxiliary_localization_tests {
    use super::{
        bulk_manager_action_label, icon_kind_label, localized_icon_wire_label,
        localized_suggestor_group_name, localized_visible_range, rules_action_label,
        rules_preview_hit_label,
    };
    use crate::business::{
        bulk_manager_panel::BulkManagerPointerHit, icons::IconKind,
        rules_wizard::RulesWizardPointerHit,
    };

    #[test]
    fn auxiliary_surfaces_use_user_facing_chinese_labels() {
        assert_eq!(icon_kind_label(IconKind::Folder, true), "文件夹");
        assert_eq!(localized_icon_wire_label("settings", true), "设置");
        assert_eq!(
            bulk_manager_action_label(BulkManagerPointerHit::Delete, true),
            "删除"
        );
        assert_eq!(
            rules_action_label(
                RulesWizardPointerHit::NextSave,
                crate::business::rules_wizard::WizardStep::Conditions,
                true,
            ),
            "下一步"
        );
        assert_eq!(localized_suggestor_group_name("Documents", true), "文档");
        assert_eq!(
            localized_suggestor_group_name("自定义分组", true),
            "自定义分组"
        );
        assert_eq!(
            localized_visible_range(5, 20, 8, true).as_deref(),
            Some("第 6–13 项，共 20 项")
        );
    }

    #[test]
    fn rules_preview_never_exposes_debug_hit_prefix() {
        let label = rules_preview_hit_label("文档.txt", 0, true);
        assert_eq!(label, "命中项 1：文档.txt");
        assert!(!label.contains("hit:"));
    }
}

#[cfg(test)]
mod panel_header_button_tests {
    use super::{
        AuxiliaryActionEmphasis, auxiliary_action_chrome, expanded_panel_aux_chrome, lerp_color,
        panel_header_button_chrome, settings_encryption_mode_button_fill_color,
        settings_theme_card_chrome, with_alpha,
    };
    use crate::PanelHeaderButtonKind;
    use bento_nano_style::tokens::{PALETTE_DARK, PALETTE_LIGHT};

    #[test]
    fn panel_header_button_chrome_matches_tauri_hover_tokens() {
        let idle = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Search, false);
        assert_eq!(idle.background, None);
        assert_eq!(idle.glyph, PALETTE_DARK.text_muted);

        let search = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Search, true);
        assert_eq!(search.background, Some(PALETTE_DARK.surface_hover));
        assert_eq!(search.glyph, PALETTE_DARK.text_primary);

        let close = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Close, true);
        assert_eq!(
            close.background,
            Some(with_alpha(PALETTE_DARK.accent_red, 0.20))
        );
        assert_eq!(close.glyph, PALETTE_DARK.accent_red);
    }

    #[test]
    fn auxiliary_action_chrome_has_distinct_primary_danger_and_disabled_hierarchy() {
        let primary = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Primary);
        let secondary = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Secondary);
        let danger = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Danger);
        let disabled = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Disabled);

        assert_eq!(primary.fill, with_alpha(PALETTE_DARK.accent_blue, 0.88));
        assert_eq!(primary.border, PALETTE_DARK.accent_blue);
        assert_eq!(primary.text, PALETTE_DARK.control_palette().on_accent);
        assert_eq!(secondary.text, PALETTE_DARK.text_primary);
        assert_eq!(danger.text, PALETTE_DARK.accent_red);
        assert_eq!(disabled.text, PALETTE_DARK.control_palette().disabled_text);
        assert_ne!(primary.fill, secondary.fill);
        assert_ne!(danger.fill, secondary.fill);

        let light_primary =
            auxiliary_action_chrome(PALETTE_LIGHT, AuxiliaryActionEmphasis::Primary);
        let light_disabled =
            auxiliary_action_chrome(PALETTE_LIGHT, AuxiliaryActionEmphasis::Disabled);
        assert_eq!(
            light_primary.text,
            PALETTE_LIGHT.control_palette().on_accent
        );
        assert_ne!(light_primary.fill, light_disabled.fill);
    }

    #[test]
    fn expanded_panel_aux_chrome_uses_live_folder_theme_tokens() {
        let dark = expanded_panel_aux_chrome(PALETTE_DARK);
        assert_eq!(
            dark.live_folder_fill,
            with_alpha(PALETTE_DARK.text_primary, 0.08)
        );
        assert_eq!(dark.live_folder_text, PALETTE_DARK.text_muted);

        let light = expanded_panel_aux_chrome(PALETTE_LIGHT);
        assert_eq!(
            light.live_folder_fill,
            with_alpha(PALETTE_LIGHT.text_primary, 0.08)
        );
        assert_eq!(light.live_folder_text, PALETTE_LIGHT.text_muted);
        assert_ne!(dark.live_folder_text, light.live_folder_text);
    }

    #[test]
    fn settings_theme_card_chrome_matches_tauri_hover_tokens() {
        let idle = settings_theme_card_chrome(PALETTE_DARK, 0.0, false);
        assert_eq!(idle.fill, PALETTE_DARK.control_palette().fill);
        assert_eq!(idle.border, None);

        let hover = settings_theme_card_chrome(PALETTE_DARK, 0.0, true);
        assert_eq!(hover.fill, PALETTE_DARK.control_palette().hover_fill);
        assert_eq!(hover.border, Some(PALETTE_DARK.control_palette().border));

        let active = settings_theme_card_chrome(PALETTE_DARK, 1.0, false);
        assert_eq!(active.fill, with_alpha(PALETTE_DARK.accent_blue, 0.10));
        assert_eq!(active.border, Some(PALETTE_DARK.accent_blue));

        let mid = settings_theme_card_chrome(PALETTE_DARK, 0.5, false);
        assert_eq!(mid.fill, lerp_color(idle.fill, active.fill, 0.5));
        assert_eq!(mid.border, Some(with_alpha(PALETTE_DARK.accent_blue, 0.5)));

        let active_hover = settings_theme_card_chrome(PALETTE_DARK, 1.0, true);
        assert_eq!(
            active_hover.fill,
            with_alpha(PALETTE_DARK.accent_blue, 0.14)
        );
        assert_eq!(active_hover.border, Some(PALETTE_DARK.accent_blue));

        let light_idle = settings_theme_card_chrome(PALETTE_LIGHT, 0.0, false);
        assert_eq!(light_idle.fill, PALETTE_LIGHT.control_palette().fill);
        assert_ne!(light_idle.fill, idle.fill);
    }

    #[test]
    fn settings_encryption_mode_button_fill_matches_tauri_hover_priority() {
        let base = with_alpha(bento_nano_style::Color::WHITE, 0.04);
        let accent = bento_nano_style::Color::from_u8(0x60, 0xA5, 0xFA, 0xFF);
        let hover = with_alpha(accent, 0.12);
        let active = with_alpha(accent, 0.18);

        assert_eq!(
            settings_encryption_mode_button_fill_color(false, false, base, hover, active),
            base
        );
        assert_eq!(
            settings_encryption_mode_button_fill_color(false, true, base, hover, active),
            hover
        );
        assert_eq!(
            settings_encryption_mode_button_fill_color(true, false, base, hover, active),
            active
        );
        assert_eq!(
            settings_encryption_mode_button_fill_color(true, true, base, hover, active),
            active
        );
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
mod morph_content_tests {
    use super::{
        PANEL_ACCENT_EDGE_THICKNESS_PX, expanded_panel_accent_clip_rect,
        morph_zen_content_to_header,
    };
    use crate::{expanded_zone_grid, zone_pill_geometry};
    use bento_nano_style::Rect;
    use bento_nano_zone::{Zone, ZoneId};

    #[test]
    fn expanded_panel_accent_clip_stays_on_panel_top_edge() {
        let panel = Rect {
            x: 64.0,
            y: 332.0,
            width: 320.0,
            height: 220.0,
        };
        let clip = expanded_panel_accent_clip_rect(panel);
        assert_eq!(clip.x, panel.x);
        assert_eq!(clip.y, panel.y);
        assert_eq!(clip.width, panel.width);
        assert_eq!(clip.height, PANEL_ACCENT_EDGE_THICKNESS_PX);
    }

    #[test]
    fn expanded_panel_accent_clip_does_not_overflow_short_panel() {
        let panel = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 1.0,
        };
        let clip = expanded_panel_accent_clip_rect(panel);
        assert_eq!(clip.height, panel.height);
    }

    #[test]
    fn morph_identity_row_has_exact_collapsed_and_expanded_endpoints() {
        let zone = Zone::new(ZoneId(1), "Benchmark Zone", 20, 30, 320, 240);
        let zen = zone_pill_geometry::pill_layout_for_zone(&zone, 10);
        let panel = expanded_zone_grid::expanded_zone_layout_for_rect(
            Rect {
                x: 20.0,
                y: 30.0,
                width: 320.0,
                height: 240.0,
            },
            10,
        );

        let collapsed = morph_zen_content_to_header(zen, &panel, 0.0);
        assert_eq!(collapsed.icon, zen.icon);
        assert_eq!(collapsed.label, zen.label);
        assert_eq!(collapsed.badge, zen.badge);

        let expanded = morph_zen_content_to_header(zen, &panel, 1.0);
        assert_eq!(expanded.icon, panel.header_icon);
        assert_eq!(expanded.badge, panel.header_badge);
        assert_eq!(expanded.rect, panel.header_band);
    }
}

#[cfg(test)]
mod collapsed_pill_badge_count_tests {
    use super::{
        collapsed_pill_display_count, format_small_count, tauri_badge_fill, tauri_zone_accent_color,
    };
    use crate::{AppState, zone_pill_geometry};
    use bento_nano_style::Color;
    use bento_nano_zone::{Zone, ZoneId};

    fn zone_with_items(id: u64, item_count: usize) -> Zone {
        let mut zone = Zone::new(ZoneId(id), format!("Zone {id}"), 40, 40, 220, 140);
        for index in 0..item_count {
            let _ = zone.add_item(format!("C:/proof/zone-{id}/item-{index}.txt"), "");
        }
        zone
    }

    #[test]
    fn badge_fill_matches_tauri_zone_accent_fallback_contract() {
        let fallback = Color::from_u8(0xFF, 0xFF, 0xFF, 0x1F);

        assert_eq!(tauri_badge_fill(None, fallback), fallback);
        assert_eq!(tauri_badge_fill(Some(""), fallback), fallback);
        assert_eq!(tauri_badge_fill(Some("#zzzzzz"), fallback), fallback);
        assert_eq!(
            tauri_badge_fill(Some("#3B82F6"), fallback),
            Color::from_u8(0x3B, 0x82, 0xF6, 0xE0)
        );
        assert_eq!(tauri_zone_accent_color(None), None);
    }

    #[test]
    fn normal_collapsed_pill_uses_item_count() {
        let mut app = AppState::new();
        app.zones.add(zone_with_items(1, 3));
        let zone = app.zones.get(ZoneId(1)).expect("zone");

        assert_eq!(collapsed_pill_display_count(&app, zone), 3);
        assert_eq!(
            format_small_count(collapsed_pill_display_count(&app, zone)),
            "3"
        );
    }

    #[test]
    fn stack_anchor_collapsed_capsule_uses_stack_member_count_for_layout_and_text() {
        let mut app = AppState::new();
        app.zones.add(zone_with_items(1, 10));
        app.zones.add(zone_with_items(2, 1));
        assert!(app.zones.stack(ZoneId(1), ZoneId(2)));

        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        let display_count = collapsed_pill_display_count(&app, anchor);
        let layout = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, display_count);
        let count_text = format_small_count(display_count);

        assert_eq!(display_count, 2);
        assert_eq!(count_text, "2");
        assert_ne!(format_small_count(anchor.items.len()), count_text);
        assert_eq!(layout.peek_visible_count, 2);
        assert!(layout.badge.width >= zone_pill_geometry::STACK_CAPSULE_BADGE_MIN_WIDTH_PX);
        assert!(layout.rect.width > 160.0);
    }
}

#[cfg(test)]
mod item_label_fit_tests {
    use super::{
        ITEM_LABEL_BASE_FONT_PX, ITEM_LABEL_BOTTOM_INSET_PX, ITEM_LABEL_MIN_FONT_PX,
        item_icon_slots_for_card, item_label_font_size_for_width, item_label_group_font_size,
        item_label_rect_for_card, item_label_text_color_for_reference, item_label_visible_name,
    };
    use bento_nano_style::Rect;
    use bento_nano_style::tokens::{PALETTE_DARK, PALETTE_LIGHT};

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn short_item_label_keeps_base_font_size() {
        assert_eq!(
            item_label_font_size_for_width("Docs", 80.0),
            ITEM_LABEL_BASE_FONT_PX
        );
    }

    #[test]
    fn item_label_uses_reference_frame_effective_size_and_rail() {
        assert_close(ITEM_LABEL_BASE_FONT_PX, 11.0);
        assert_close(ITEM_LABEL_BOTTOM_INSET_PX, 8.0);
    }

    #[test]
    fn long_item_label_shrinks_instead_of_relying_on_ellipsis() {
        let got = item_label_font_size_for_width("Roxy Browser", 58.0);

        assert!(
            got < ITEM_LABEL_BASE_FONT_PX,
            "long item labels must shrink before drawing, got {got}"
        );
        assert!(
            got >= ITEM_LABEL_MIN_FONT_PX,
            "item labels must keep the shared readability floor, got {got}"
        );
    }

    #[test]
    fn extremely_narrow_item_label_bottoms_out_at_readability_floor() {
        assert_eq!(
            item_label_font_size_for_width("very-long-file-name.txt", 20.0),
            ITEM_LABEL_MIN_FONT_PX
        );
    }

    #[test]
    fn item_grid_uses_one_uniform_font_size_for_every_visible_label() {
        let labels = [("Docs", 80.0), ("DB Browser (SQLCipher)", 80.0)];
        let group = item_label_group_font_size(labels.into_iter());

        assert_eq!(group, ITEM_LABEL_MIN_FONT_PX);
        assert!(group < item_label_font_size_for_width("Docs", 80.0));
    }

    #[test]
    fn shortcut_extensions_are_removed_before_fit() {
        assert_eq!(item_label_visible_name("Project.lnk"), "Project");
        assert_eq!(item_label_visible_name("Docs.URL"), "Docs");
        assert_eq!(item_label_visible_name("archive.txt"), "archive.txt");
    }

    #[test]
    fn item_label_color_uses_tauri_secondary_text_ink() {
        assert_eq!(
            item_label_text_color_for_reference(PALETTE_DARK),
            PALETTE_DARK.text_secondary
        );
        assert_eq!(
            item_label_text_color_for_reference(PALETTE_LIGHT),
            PALETTE_LIGHT.text_secondary
        );
    }

    #[test]
    fn standard_item_label_uses_reference_lower_text_rail() {
        let card = Rect {
            x: 10.0,
            y: 20.0,
            width: 88.0,
            height: 78.0,
        };

        let label = item_label_rect_for_card(card, 1.0, ITEM_LABEL_BASE_FONT_PX);
        let expected_h = ITEM_LABEL_BASE_FONT_PX * 1.4;

        assert_close(label.x, 14.0);
        assert_close(label.width, 80.0);
        assert_close(label.height, expected_h);
        assert_close(
            label.y,
            card.bottom() - expected_h - ITEM_LABEL_BOTTOM_INSET_PX,
        );
    }

    #[test]
    fn scaled_item_label_keeps_bottom_inset_with_card_transform() {
        let card = Rect {
            x: 4.0,
            y: 6.0,
            width: 120.0,
            height: 90.0,
        };
        let scale = 1.25;

        let label = item_label_rect_for_card(card, scale, ITEM_LABEL_BASE_FONT_PX);
        let expected_h = ITEM_LABEL_BASE_FONT_PX * 1.4 * scale;

        assert_close(label.x, card.x + 4.0 * scale);
        assert_close(label.width, card.width - 8.0 * scale);
        assert_close(label.height, expected_h);
        assert_close(
            label.y,
            card.bottom() - expected_h - ITEM_LABEL_BOTTOM_INSET_PX * scale,
        );
    }

    #[test]
    fn standard_item_icon_uses_36px_container_and_24px_render_slot() {
        let card = Rect {
            x: 10.0,
            y: 20.0,
            width: 88.0,
            height: 78.0,
        };

        let (container, render) = item_icon_slots_for_card(card, false, 1.0);

        assert_close(container.x, 36.0);
        assert_close(container.y, 28.0);
        assert_close(container.width, 36.0);
        assert_close(container.height, 36.0);
        assert_close(render.x, 42.0);
        assert_close(render.y, 34.0);
        assert_close(render.width, 24.0);
        assert_close(render.height, 24.0);
    }

    #[test]
    fn wide_item_icon_uses_28px_container_and_20px_render_slot() {
        let card = Rect {
            x: 5.0,
            y: 10.0,
            width: 200.0,
            height: 78.0,
        };

        let (container, render) = item_icon_slots_for_card(card, true, 1.0);

        assert_close(container.x, 91.0);
        assert_close(container.y, 18.0);
        assert_close(container.width, 28.0);
        assert_close(container.height, 28.0);
        assert_close(render.x, 95.0);
        assert_close(render.y, 22.0);
        assert_close(render.width, 20.0);
        assert_close(render.height, 20.0);
    }
}

#[cfg(test)]
mod g5_pill_title_shrink_tests {
    use super::{
        PILL_TITLE_MIN_FONT_PX, shrink_font_to_fit, text_width_with_tracking,
        title_shrink_signature,
    };

    /// Linear width model: each glyph is `0.6 * font_px` wide, `len` glyphs.
    /// Monotone in font size, matching the `shrink_font_to_fit` contract.
    fn measure(len: f32) -> impl FnMut(f32) -> f32 {
        move |size: f32| len * size * 0.6
    }

    #[test]
    fn returns_base_when_label_already_fits() {
        // 5 glyphs at 14px → 42 DIPs wide; avail 100 ⇒ no shrink, keep base.
        let got = shrink_font_to_fit(14.0, 100.0, measure(5.0));
        assert!((got - 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn shrinks_to_largest_fitting_size_no_floor() {
        // 20 glyphs; base 16px → 192 wide; avail 130. The stepper returns the
        // largest whole-px size whose run fits, well above the floor.
        let avail = 130.0_f32;
        let len = 20.0_f32;
        let mut m = measure(len);
        let got = shrink_font_to_fit(16.0, avail, measure(len));
        assert!(got < 16.0, "must have shrunk from base, got {got}");
        assert!(
            got > PILL_TITLE_MIN_FONT_PX,
            "must not bottom out, got {got}"
        );
        // The resolved size genuinely fits and 1px larger would not (the
        // stepper's contract: largest fitting whole-px size).
        assert!(m(got) <= avail, "resolved must fit: {} > {avail}", m(got));
        assert!(m(got + 1.0) > avail, "one px larger must overflow");
    }

    #[test]
    fn bottoms_out_at_floor_when_nothing_fits() {
        // A pathologically long label in a tiny width never fits ⇒ floor (8px),
        // while the draw path still emits the complete text (Tauri v7).
        let got = shrink_font_to_fit(16.0, 4.0, measure(50.0));
        assert!((got - PILL_TITLE_MIN_FONT_PX).abs() < f32::EPSILON);
    }

    #[test]
    fn base_below_floor_is_clamped_up() {
        // A base smaller than the floor never returns below the floor.
        let got = shrink_font_to_fit(4.0, 1000.0, measure(1.0));
        assert!(got >= PILL_TITLE_MIN_FONT_PX);
    }

    #[test]
    fn signature_is_stable_and_discriminates() {
        let a = title_shrink_signature("Documents", 120.0, 14.0, 500, 0.3);
        let b = title_shrink_signature("Documents", 120.0, 14.0, 500, 0.3);
        assert_eq!(a, b, "same inputs must hash identically (cache hit)");
        // Any typography input changing should (almost always) change it.
        assert_ne!(
            a,
            title_shrink_signature("Downloads", 120.0, 14.0, 500, 0.3)
        );
        assert_ne!(a, title_shrink_signature("Documents", 90.0, 14.0, 500, 0.3));
        assert_ne!(
            a,
            title_shrink_signature("Documents", 120.0, 11.0, 500, 0.3)
        );
        assert_ne!(
            a,
            title_shrink_signature("Documents", 120.0, 14.0, 600, 0.3)
        );
        assert_ne!(
            a,
            title_shrink_signature("Documents", 120.0, 14.0, 500, 0.0)
        );
    }

    #[test]
    fn shrink_measurement_includes_letter_spacing_advance() {
        let units = "Compiler".encode_utf16().count();
        let tracking = 0.3;
        let avail = 75.0;
        let mut measure_with_tracking =
            |size: f32| text_width_with_tracking(size * 4.625, units, tracking);

        let got = shrink_font_to_fit(16.0, avail, &mut measure_with_tracking);

        assert_eq!(got, 15.0);
        assert!(measure_with_tracking(got) <= avail);
        assert!(measure_with_tracking(got + 1.0) > avail);
    }

    #[test]
    fn stack_capsule_title_can_shrink_to_tight_grid_column() {
        // V21-C6 — a two-member 220px StackCapsule leaves a tight title column.
        // The stack title must shrink before the floor; this models the full
        // "Benchmark Zone 3" title at the
        // stack token base size (13px).
        let title_len = "Benchmark Zone 3".chars().count() as f32;
        let mut m = |size: f32| title_len * size * 0.52;
        let got = shrink_font_to_fit(13.0, 69.0, &mut m);

        assert!(got < 13.0, "stack title must shrink from base, got {got}");
        assert!(
            got >= PILL_TITLE_MIN_FONT_PX,
            "stack title must respect the shared floor, got {got}"
        );
        assert!(m(got) <= 69.0, "resolved stack title width must fit");
    }
}

#[cfg(test)]
mod p1_caret_blink_tests {
    use super::settings_caret_on;

    /// P1 (#7 fix wave 2026-06-01) — the caret is ON for the first ~530ms
    /// half-period and OFF for the next, toggling at the Windows blink cadence.
    /// Pure function of `now_ms` (no state) so it's directly unit-testable.
    #[test]
    fn caret_blinks_on_530ms_half_period() {
        // First half-period (0..530) → ON.
        assert!(settings_caret_on(0));
        assert!(settings_caret_on(265));
        assert!(settings_caret_on(529));
        // Second half-period (530..1060) → OFF.
        assert!(!settings_caret_on(530));
        assert!(!settings_caret_on(800));
        assert!(!settings_caret_on(1059));
        // Third half-period (1060..1590) → ON again (period wraps).
        assert!(settings_caret_on(1060));
        assert!(settings_caret_on(1500));
        // The phase alternates every 530ms with no gaps.
        for k in 0..16u32 {
            let mid = k * 530 + 100;
            assert_eq!(settings_caret_on(mid), k % 2 == 0, "half-period {k} phase");
        }
    }
}

#[cfg(test)]
mod m6c_effect_geometry_tests {
    use super::{
        chromatic_split_offsets, crisp_shadow_rect, lerp_neon_layer, neon_glow_rect,
        scanline_band_count, stack_bloom_active_pulse, stack_bloom_active_transition_t,
    };
    use bento_nano_style::{Color, Rect, Shadow};

    #[test]
    fn zone_shadow_suppresses_blur_but_preserves_crisp_ring() {
        let base = Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 40.0,
        };
        assert_eq!(
            crisp_shadow_rect(base, Shadow::drop(0.0, 12.0, 48.0, Color::BLACK)),
            None,
            "blurred geometry must not become a broad solid halo"
        );
        let ring = Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::rgba(0.2, 0.5, 1.0, 0.4),
        };
        assert_eq!(
            crisp_shadow_rect(base, ring),
            Some(Rect {
                x: 9.0,
                y: 19.0,
                width: 102.0,
                height: 42.0,
            })
        );
    }

    #[test]
    fn stack_bloom_active_scale_transition_settles_by_180ms() {
        assert_eq!(stack_bloom_active_transition_t(1_000, 1_000), 0.0);
        assert!((stack_bloom_active_transition_t(1_090, 1_000) - 0.5).abs() < 1e-6);
        assert_eq!(stack_bloom_active_transition_t(1_180, 1_000), 1.0);
        assert_eq!(stack_bloom_active_transition_t(2_000, 1_000), 1.0);
    }

    #[test]
    fn stack_bloom_active_pulse_keeps_tauri_bounds_and_many_member_static_rule() {
        assert_eq!(stack_bloom_active_pulse(1_000, 1_000, false), (5.5, 0.16));
        assert_eq!(stack_bloom_active_pulse(1_600, 1_000, false), (5.5, 0.16));
        let peak = stack_bloom_active_pulse(2_350, 1_000, false);
        assert!((peak.0 - 7.0).abs() < 1e-6);
        assert!((peak.1 - 0.22).abs() < 1e-6);
        let wrapped = stack_bloom_active_pulse(3_100, 1_000, false);
        assert!((wrapped.0 - 5.5).abs() < 1e-6);
        assert!((wrapped.1 - 0.16).abs() < 1e-6);
        assert_eq!(stack_bloom_active_pulse(2_350, 1_000, true), (4.0, 0.18));
    }

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
mod frosted_backdrop_tests {
    use super::{
        FROSTED_BACKDROP_DOWNSAMPLE, FROSTED_BACKDROP_SATURATION_DARK,
        FROSTED_BACKDROP_SATURATION_LIGHT, FROSTED_BACKDROP_STDDEV, FROSTED_FALLBACK_MIN_ALPHA,
        ORDINARY_LARGE_PILL_SHADOW_OPACITY, ORDINARY_MEDIUM_PILL_SHADOW_OPACITY,
        STACK_CAPSULE_BLOOMED_OPACITY, STACK_CAPSULE_BLOOMED_RECEDES_MS,
        STACK_CAPSULE_BLOOMED_SCALE, STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS,
        STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE, STACK_CAPSULE_EMERGE_START_SCALE,
        collapsed_zen_surface_color, d2d_gradient_stop, fade_color,
        frosted_backdrop_saturation_changed, frosted_backdrop_saturation_for_palette,
        frosted_backdrop_saturation_recapture_needed, frosted_fallback_underlay,
        frosted_group_backdrop_opacity, lerp_color, lerp_shadow_stack, opaque_auxiliary_surface,
        ordinary_zone_pill_chrome_radius, ordinary_zone_pill_shadow_stack,
        scale_about_rect_center_matrix, scale_rect_about_center, stack_capsule_badge_chrome,
        stack_capsule_bloom_shadow_stack, stack_capsule_bloom_text_transform,
        stack_capsule_bloom_visual, stack_capsule_bloom_visual_for_app,
        stack_capsule_bloomed_target_shadow_stack, stack_capsule_emerge_visual,
        stack_capsule_glass_sheen_colors, stack_capsule_has_preview,
        stack_capsule_hover_border_color, stack_capsule_hover_shadow_stack,
        stack_capsule_hover_target_shadow_stack, stack_capsule_hover_translate_y,
        stack_capsule_is_locked, stack_capsule_locked_opacity,
        stack_capsule_presented_emerge_visual, stack_capsule_preview_indicator_width,
        stack_capsule_preview_shadow_stack, stack_capsule_sheen_gradient_props,
        stack_capsule_show_preview_indicator, stack_capsule_visual_shadow_stack,
        title_shrink_signature, translate_rect, vertical_gradient_props, with_alpha,
    };
    use crate::AppState;
    use crate::business::stack_tray::StackTrayState;
    use crate::business::zen_capsule::CapsuleSize;
    use crate::zone_pill_geometry;
    use bento_nano_platform::WindowKind;
    use bento_nano_style::{BorderRadius, Color, Rect, Shadow, ShadowStack};
    use bento_nano_zone::{Zone, ZoneId};

    #[test]
    fn shared_frosted_backdrop_tracks_tauri_zen_blur_under_memory_budget() {
        assert_eq!(FROSTED_BACKDROP_DOWNSAMPLE, 4);
        assert!((FROSTED_BACKDROP_STDDEV - 5.0).abs() < f32::EPSILON);
        assert!((FROSTED_BACKDROP_SATURATION_DARK - 1.6).abs() < f32::EPSILON);
        assert!((FROSTED_BACKDROP_SATURATION_LIGHT - 1.3).abs() < f32::EPSILON);
        assert!(
            (frosted_backdrop_saturation_for_palette(bento_nano_style::tokens::PALETTE_DARK) - 1.6)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (frosted_backdrop_saturation_for_palette(bento_nano_style::tokens::PALETTE_LIGHT)
                - 1.3)
                .abs()
                < f32::EPSILON
        );
        assert!(!frosted_backdrop_saturation_changed(1.6, 1.6));
        assert!(frosted_backdrop_saturation_changed(1.6, 1.3));
        assert!(frosted_backdrop_saturation_recapture_needed(
            WindowKind::Main,
            1.6,
            1.3
        ));
        assert!(!frosted_backdrop_saturation_recapture_needed(
            WindowKind::Settings,
            1.6,
            1.3
        ));
    }

    #[test]
    fn degraded_frosted_surface_stays_dense_but_translucent() {
        let zen = bento_nano_style::tokens::PALETTE_DARK.surface_zen;
        let underlay = frosted_fallback_underlay(zen).expect("zen needs fallback underlay");
        let composed_alpha = zen.a + underlay.a * (1.0 - zen.a);
        assert!((composed_alpha - FROSTED_FALLBACK_MIN_ALPHA).abs() < f32::EPSILON);
        assert!(composed_alpha > zen.a);
        assert!(composed_alpha < 1.0);
        assert_eq!(underlay.r, zen.r);
        assert_eq!(underlay.g, zen.g);
        assert_eq!(underlay.b, zen.b);

        let already_dense = with_alpha(zen, FROSTED_FALLBACK_MIN_ALPHA);
        assert!(frosted_fallback_underlay(already_dense).is_none());
    }

    #[test]
    fn auxiliary_panel_fallback_is_solid_while_rounded_corners_stay_clear() {
        let token = Color::rgba(0.05, 0.06, 0.08, 0.82);
        let fallback = opaque_auxiliary_surface(token);
        assert_eq!(fallback.r, token.r);
        assert_eq!(fallback.g, token.g);
        assert_eq!(fallback.b, token.b);
        assert_eq!(fallback.a, 1.0);
    }

    #[test]
    fn frosted_group_opacity_preserves_css_layer_coefficients() {
        let tint_alpha = 0.55;
        let group_opacity = 0.50;
        let faded_tint_alpha = tint_alpha * group_opacity;
        let backdrop_opacity = frosted_group_backdrop_opacity(tint_alpha, group_opacity);

        assert!(
            ((1.0 - faded_tint_alpha) * backdrop_opacity - group_opacity * (1.0 - tint_alpha))
                .abs()
                < 1e-6
        );
        assert!(
            ((1.0 - faded_tint_alpha) * (1.0 - backdrop_opacity) - (1.0 - group_opacity)).abs()
                < 1e-6
        );
        assert_eq!(frosted_group_backdrop_opacity(tint_alpha, 0.0), 0.0);
        assert_eq!(frosted_group_backdrop_opacity(tint_alpha, 1.0), 1.0);
        assert_eq!(frosted_group_backdrop_opacity(1.0, 0.5), 0.0);
    }

    /// Frosted-backdrop — the capsule↔panel morph cross-fades
    /// `surface_zen → surface_expanded` along the shared morph. Pin the endpoints and
    /// the midpoint, INCLUDING the alpha channel. The endpoint must match the
    /// settled expanded-panel renderer so the morph cannot over-darken before
    /// the steady panel path takes over.
    #[test]
    fn lerp_color_endpoints_and_midpoint() {
        // surface_zen (#121218 @ 0x8C) → surface_expanded (#0C0C12 @ 0xD1), the
        // exact Tauri dark tokens the morph blends between.
        let zen = Color::from_u8(0x12, 0x12, 0x18, 0x8C);
        let expanded = Color::from_u8(0x0C, 0x0C, 0x12, 0xD1);

        // t = 0 → exactly the start colour.
        let at0 = lerp_color(zen, expanded, 0.0);
        assert_eq!(at0, zen);
        // t = 1 → exactly the end colour.
        let at1 = lerp_color(zen, expanded, 1.0);
        assert_eq!(at1, expanded);
        assert_eq!(at1, bento_nano_style::tokens::PALETTE_DARK.surface_expanded);

        // t = 0.5 → per-channel midpoint, alpha included.
        let mid = lerp_color(zen, expanded, 0.5);
        let eps = 1e-6_f32;
        assert!((mid.r - (zen.r + expanded.r) * 0.5).abs() < eps);
        assert!((mid.g - (zen.g + expanded.g) * 0.5).abs() < eps);
        assert!((mid.b - (zen.b + expanded.b) * 0.5).abs() < eps);
        assert!((mid.a - (zen.a + expanded.a) * 0.5).abs() < eps);
        // The alpha genuinely moves (0x8C/255 .. 0xD1/255 midpoint).
        let expected_a = (0x8C as f32 / 255.0 + 0xD1 as f32 / 255.0) * 0.5;
        assert!((mid.a - expected_a).abs() < eps);
    }

    /// Out-of-range `t` clamps to `[0, 1]` so malformed/transient state can
    /// never over/under-saturate the morph tint.
    #[test]
    fn lerp_color_clamps_t() {
        let a = Color::rgba(0.0, 0.0, 0.0, 0.0);
        let b = Color::rgba(1.0, 1.0, 1.0, 1.0);
        // t < 0 → clamp to start.
        assert_eq!(lerp_color(a, b, -0.5), a);
        // t > 1 → clamp to end.
        assert_eq!(lerp_color(a, b, 1.5), b);
    }

    #[test]
    fn vertical_gradient_props_follow_rect_top_to_bottom() {
        let rect = Rect {
            x: 12.0,
            y: 34.0,
            width: 160.0,
            height: 48.0,
        };
        let props = vertical_gradient_props(rect);
        assert_eq!(props.startPoint.x, rect.x);
        assert_eq!(props.startPoint.y, rect.y);
        assert_eq!(props.endPoint.x, rect.x);
        assert_eq!(props.endPoint.y, rect.bottom());
    }

    #[test]
    fn stack_capsule_gradient_props_follow_tauri_135deg_contract() {
        let rect = Rect {
            x: 12.0,
            y: 34.0,
            width: 160.0,
            height: 48.0,
        };
        let props = stack_capsule_sheen_gradient_props(rect);
        assert_eq!(props.startPoint.x, rect.x);
        assert_eq!(props.startPoint.y, rect.y);
        assert_eq!(props.endPoint.x, rect.right());
        assert_eq!(props.endPoint.y, rect.bottom());
    }

    #[test]
    fn d2d_gradient_stop_clamps_position_and_keeps_rgba() {
        let color = Color::from_u8(0x12, 0x16, 0x22, 0xD1);
        let stop = d2d_gradient_stop(1.5, color);
        assert_eq!(stop.position, 1.0);
        assert_eq!(stop.color.r, color.r);
        assert_eq!(stop.color.g, color.g);
        assert_eq!(stop.color.b, color.b);
        assert_eq!(stop.color.a, color.a);
    }

    #[test]
    fn collapsed_zen_surface_ignores_hover_to_match_tauri_css() {
        let idle = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 0.0);
        let hover = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 1.0);
        let overshoot = collapsed_zen_surface_color(bento_nano_style::tokens::PALETTE_DARK, 2.0);
        assert_eq!(idle, bento_nano_style::tokens::PALETTE_DARK.surface_zen);
        assert_eq!(hover, idle);
        assert_eq!(overshoot, idle);
    }

    #[test]
    fn ordinary_pill_shadow_attenuates_medium_and_large_without_changing_geometry() {
        let idle = bento_nano_style::tokens::SHADOW.zen;
        let medium = ordinary_zone_pill_shadow_stack(CapsuleSize::Medium, idle);
        let large = ordinary_zone_pill_shadow_stack(CapsuleSize::Large, idle);

        assert_eq!(medium.len(), idle.len());
        assert_eq!(medium.inner().offset_x, idle.inner().offset_x);
        assert_eq!(medium.inner().offset_y, idle.inner().offset_y);
        assert_eq!(medium.inner().blur, idle.inner().blur);
        assert_eq!(medium.inner().spread, idle.inner().spread);
        assert_eq!(medium.outer().offset_x, idle.outer().offset_x);
        assert_eq!(medium.outer().offset_y, idle.outer().offset_y);
        assert_eq!(medium.outer().blur, idle.outer().blur);
        assert_eq!(medium.outer().spread, idle.outer().spread);
        assert!(
            (medium.inner().color.a - idle.inner().color.a * ORDINARY_MEDIUM_PILL_SHADOW_OPACITY)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (medium.outer().color.a - idle.outer().color.a * ORDINARY_MEDIUM_PILL_SHADOW_OPACITY)
                .abs()
                < f32::EPSILON
        );

        assert_eq!(
            ordinary_zone_pill_shadow_stack(CapsuleSize::Small, idle),
            idle
        );
        assert_eq!(large.len(), idle.len());
        assert_eq!(large.inner().offset_x, idle.inner().offset_x);
        assert_eq!(large.inner().offset_y, idle.inner().offset_y);
        assert_eq!(large.inner().blur, idle.inner().blur);
        assert_eq!(large.inner().spread, idle.inner().spread);
        assert_eq!(large.outer().offset_x, idle.outer().offset_x);
        assert_eq!(large.outer().offset_y, idle.outer().offset_y);
        assert_eq!(large.outer().blur, idle.outer().blur);
        assert_eq!(large.outer().spread, idle.outer().spread);
        assert!(
            (large.inner().color.a - idle.inner().color.a * ORDINARY_LARGE_PILL_SHADOW_OPACITY)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (large.outer().color.a - idle.outer().color.a * ORDINARY_LARGE_PILL_SHADOW_OPACITY)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn ordinary_pill_chrome_radius_caps_at_half_the_visible_height() {
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 160.0,
            height: 48.0,
        };
        assert_eq!(
            ordinary_zone_pill_chrome_radius(rect, BorderRadius::all(24.0)),
            BorderRadius::all(24.0)
        );
        let large_rect = Rect {
            width: 220.0,
            height: 56.0,
            ..rect
        };
        assert_eq!(
            ordinary_zone_pill_chrome_radius(large_rect, BorderRadius::all(28.0)),
            BorderRadius::all(28.0)
        );
        let tiny = Rect {
            height: 6.0,
            ..rect
        };
        assert_eq!(
            ordinary_zone_pill_chrome_radius(tiny, BorderRadius::all(24.0)),
            BorderRadius::all(3.0)
        );
    }

    #[test]
    fn morph_shadow_stack_preserves_both_endpoints() {
        let from = ShadowStack::one(Shadow::drop(0.0, 0.0, 0.0, Color::rgba(1.0, 0.0, 0.0, 0.5)));
        let to = ShadowStack::two(
            Shadow::drop(1.0, 2.0, 0.0, Color::rgba(0.0, 1.0, 0.0, 0.4)),
            Shadow::drop(3.0, 4.0, 0.0, Color::rgba(0.0, 0.0, 1.0, 0.3)),
        );

        let start = lerp_shadow_stack(from, to, 0.0);
        assert_eq!(start.inner(), from.inner());
        assert_eq!(start.outer().color.a, 0.0);
        assert_eq!(lerp_shadow_stack(from, to, 1.0), to);
    }

    #[test]
    fn stack_capsule_sheen_matches_tauri_stackwrapper_alpha_stops() {
        let (start, end) = stack_capsule_glass_sheen_colors();
        assert_eq!(start.r, 1.0);
        assert_eq!(start.g, 1.0);
        assert_eq!(start.b, 1.0);
        assert_eq!(end.r, 1.0);
        assert_eq!(end.g, 1.0);
        assert_eq!(end.b, 1.0);
        assert!((start.a - 0.08).abs() < f32::EPSILON);
        assert!((end.a - 0.02).abs() < f32::EPSILON);
    }

    #[test]
    fn stack_capsule_locked_chrome_matches_tauri_css() {
        assert!((stack_capsule_locked_opacity(false) - 1.0).abs() < f32::EPSILON);
        assert!((stack_capsule_locked_opacity(true) - 0.9).abs() < f32::EPSILON);

        let pal = bento_nano_style::tokens::PALETTE_DARK;
        let unlocked = stack_capsule_badge_chrome(pal, false);
        assert_eq!(unlocked.fill, with_alpha(pal.text_primary, 0.08));
        assert_eq!(unlocked.text, pal.text_primary);

        let locked = stack_capsule_badge_chrome(pal, true);
        assert_eq!(locked.fill, Color::from_u8(0xF5, 0x9E, 0x0B, 0x24));
        assert_eq!(locked.text, Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF));
    }

    #[test]
    fn stack_capsule_locked_rule_matches_tauri_any_zone_locked() {
        let mut app = AppState::new();
        app.zones
            .add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
        app.zones
            .add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));
        app.zones
            .add(Zone::new(ZoneId(3), "child-b", 100, 80, 120, 90));
        let member_ids = [ZoneId(1), ZoneId(2), ZoneId(3)];

        {
            let anchor = app.zones.get(ZoneId(1)).expect("anchor");
            assert!(!stack_capsule_is_locked(&app, anchor, &member_ids));
        }

        app.zones.get_mut(ZoneId(2)).expect("child").locked = true;
        {
            let anchor = app.zones.get(ZoneId(1)).expect("anchor");
            assert!(stack_capsule_is_locked(&app, anchor, &member_ids));
        }

        app.zones.get_mut(ZoneId(2)).expect("child").locked = false;
        app.zones.get_mut(ZoneId(1)).expect("anchor").locked = true;
        let anchor = app.zones.get(ZoneId(1)).expect("anchor");
        assert!(stack_capsule_is_locked(&app, anchor, &member_ids));
    }

    #[test]
    fn stack_capsule_preview_rule_matches_tauri_has_preview_state() {
        let mut app = AppState::new();
        app.zones
            .add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
        app.zones
            .add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));

        assert!(!stack_capsule_has_preview(&app, ZoneId(1)));
        app.stack_tray
            .borrow_mut()
            .replace(StackTrayState::new(ZoneId(1), ZoneId(1)));
        assert!(!stack_capsule_has_preview(&app, ZoneId(1)));

        app.stack_tray
            .borrow_mut()
            .replace(StackTrayState::new(ZoneId(1), ZoneId(2)));
        assert!(stack_capsule_has_preview(&app, ZoneId(1)));
        assert!(!stack_capsule_has_preview(&app, ZoneId(2)));
    }

    #[test]
    fn stack_capsule_preview_chrome_matches_tauri_css() {
        let preview = stack_capsule_preview_shadow_stack();
        assert_eq!(preview.inner().offset_x, 0.0);
        assert_eq!(preview.inner().offset_y, 18.0);
        assert_eq!(preview.inner().blur, 42.0);
        assert_eq!(preview.inner().spread, 0.0);
        assert!((preview.inner().color.a - 0.24).abs() < f32::EPSILON);

        let ring = preview.outer();
        assert_eq!(ring.offset_x, 0.0);
        assert_eq!(ring.offset_y, 0.0);
        assert_eq!(ring.blur, 0.0);
        assert_eq!(ring.spread, 1.0);
        assert_eq!(ring.color, Color::from_u8(0x3B, 0x82, 0xF6, 0x6B));

        assert!(stack_capsule_show_preview_indicator(true, 0.0));
        assert!(!stack_capsule_show_preview_indicator(true, 0.01));
        assert!(!stack_capsule_show_preview_indicator(false, 0.0));

        let zh_width = stack_capsule_preview_indicator_width("预览中");
        let en_width = stack_capsule_preview_indicator_width("Preview open");
        assert!(zh_width >= 34.0);
        assert!(en_width <= 82.0);

        let idle = bento_nano_style::tokens::SHADOW.zen;
        assert_eq!(
            stack_capsule_visual_shadow_stack(idle, 0.0, 0.0, true),
            preview
        );
        assert_eq!(
            stack_capsule_visual_shadow_stack(idle, 1.0, 0.0, false),
            stack_capsule_hover_shadow_stack(idle, 1.0)
        );
    }

    #[test]
    fn stack_capsule_hover_lift_matches_tauri_translate_y_contract() {
        assert_eq!(stack_capsule_hover_translate_y(-1.0), 0.0);
        assert_eq!(stack_capsule_hover_translate_y(0.0), 0.0);
        assert_eq!(stack_capsule_hover_translate_y(0.5), -0.5);
        assert_eq!(stack_capsule_hover_translate_y(1.0), -1.0);
        assert_eq!(stack_capsule_hover_translate_y(2.0), -1.0);

        let rect = Rect {
            x: 12.0,
            y: 34.0,
            width: 160.0,
            height: 48.0,
        };
        let lifted = translate_rect(rect, 0.0, stack_capsule_hover_translate_y(1.0));
        assert_eq!(lifted.x, rect.x);
        assert_eq!(lifted.y, rect.y - 1.0);
        assert_eq!(lifted.width, rect.width);
        assert_eq!(lifted.height, rect.height);
    }

    #[test]
    fn stack_capsule_hover_border_reaches_tauri_literal_white_alpha() {
        let idle = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 0.0);
        let mid = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 0.5);
        let hover = stack_capsule_hover_border_color(bento_nano_style::tokens::PALETTE_DARK, 1.0);

        assert_eq!(idle, bento_nano_style::tokens::PALETTE_DARK.border_zen);
        assert_eq!(hover.r, 1.0);
        assert_eq!(hover.g, 1.0);
        assert_eq!(hover.b, 1.0);
        assert!((hover.a - 0.18).abs() < f32::EPSILON);
        assert!((mid.a - (idle.a + 0.18) * 0.5).abs() < 1e-6);
    }

    #[test]
    fn stack_capsule_hover_shadow_reaches_tauri_hover_box_shadow() {
        let idle_shadow = bento_nano_style::tokens::SHADOW.zen;
        assert_eq!(
            stack_capsule_hover_shadow_stack(idle_shadow, 0.0),
            idle_shadow
        );

        let target = stack_capsule_hover_target_shadow_stack();
        let hover = stack_capsule_hover_shadow_stack(idle_shadow, 1.0);
        assert_eq!(hover, target);

        let dark_drop = hover.inner();
        assert_eq!(dark_drop.offset_x, 0.0);
        assert_eq!(dark_drop.offset_y, 18.0);
        assert_eq!(dark_drop.blur, 42.0);
        assert_eq!(dark_drop.spread, 0.0);
        assert!((dark_drop.color.a - 0.24).abs() < f32::EPSILON);

        let white_ring = hover.outer();
        assert_eq!(white_ring.offset_x, 0.0);
        assert_eq!(white_ring.offset_y, 0.0);
        assert_eq!(white_ring.blur, 0.0);
        assert_eq!(white_ring.spread, 1.0);
        assert_eq!(white_ring.color.r, 1.0);
        assert_eq!(white_ring.color.g, 1.0);
        assert_eq!(white_ring.color.b, 1.0);
        assert!((white_ring.color.a - 0.04).abs() < f32::EPSILON);
    }

    #[test]
    fn stack_capsule_bloom_recedes_to_tauri_scale_opacity_by_180ms() {
        let member_count = 2;
        let reveal_ms = crate::business::stack_tray::stack_bloom_reveal_duration_ms(member_count);
        let cutoff_progress = STACK_CAPSULE_BLOOMED_RECEDES_MS / reveal_ms as f32;

        let start = stack_capsule_bloom_visual(0.0, member_count, false);
        assert_eq!(start.recede_t, 0.0);
        assert_eq!(start.scale, 1.0);
        assert_eq!(start.opacity, 1.0);

        let at_cutoff = stack_capsule_bloom_visual(cutoff_progress, member_count, false);
        assert!((at_cutoff.recede_t - 1.0).abs() < 1e-6);
        assert!((at_cutoff.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
        assert!((at_cutoff.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);

        let settled = stack_capsule_bloom_visual(1.0, member_count, false);
        assert!((settled.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
        assert!((settled.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);
    }

    #[test]
    fn stack_capsule_emerge_matches_tauri_spring_keyframes() {
        let start = stack_capsule_emerge_visual(0.0);
        assert_eq!(start.scale, STACK_CAPSULE_EMERGE_START_SCALE);
        assert_eq!(start.opacity, 0.0);

        let overshoot = stack_capsule_emerge_visual(0.60);
        assert!((overshoot.scale - STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE).abs() < 1e-6);
        assert_eq!(overshoot.opacity, 1.0);

        let settled = stack_capsule_emerge_visual(1.0);
        assert_eq!(settled.scale, 1.0);
        assert_eq!(settled.opacity, 1.0);
    }

    #[test]
    fn stack_capsule_first_native_present_is_visible_without_changing_keyframe_endpoint() {
        let first = stack_capsule_presented_emerge_visual(0.0);
        assert!(first.opacity > 0.0);
        assert!(first.opacity < 1.0);
        assert!(first.scale > STACK_CAPSULE_EMERGE_START_SCALE);
        const {
            assert!(
                STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS > 0.0
                    && STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS < 0.25
            );
        }

        let settled = stack_capsule_presented_emerge_visual(1.0);
        assert_eq!(settled.scale, 1.0);
        assert_eq!(settled.opacity, 1.0);
    }

    #[test]
    fn stack_capsule_bloom_visual_requires_explicit_bloom_state() {
        let app = AppState::new();
        app.hovered_zone.set(Some(ZoneId(1)));

        let held_after_drop = stack_capsule_bloom_visual_for_app(&app, ZoneId(1), 4);
        assert_eq!(held_after_drop.recede_t, 0.0);
        assert_eq!(held_after_drop.scale, 1.0);
        assert_eq!(held_after_drop.opacity, 1.0);

        app.stack_bloom_anchor.set(Some(ZoneId(1)));
        app.stack_bloom_progress.set(1.0);
        let bloomed = stack_capsule_bloom_visual_for_app(&app, ZoneId(1), 4);
        assert_eq!(bloomed.recede_t, 1.0);
        assert_eq!(bloomed.scale, STACK_CAPSULE_BLOOMED_SCALE);
        assert_eq!(bloomed.opacity, STACK_CAPSULE_BLOOMED_OPACITY);
    }

    #[test]
    fn stack_capsule_bloom_leave_restores_identity_without_anchor_pop() {
        let leaving_start = stack_capsule_bloom_visual(0.0, 5, true);
        assert!((leaving_start.scale - STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-6);
        assert!((leaving_start.opacity - STACK_CAPSULE_BLOOMED_OPACITY).abs() < 1e-6);

        let leaving_done = stack_capsule_bloom_visual(1.0, 5, true);
        assert_eq!(leaving_done.recede_t, 0.0);
        assert_eq!(leaving_done.scale, 1.0);
        assert_eq!(leaving_done.opacity, 1.0);
    }

    #[test]
    fn stack_capsule_child_rects_scale_about_capsule_center() {
        let capsule = Rect {
            x: 100.0,
            y: 200.0,
            width: 220.0,
            height: 52.0,
        };
        let child = Rect {
            x: 112.0,
            y: 214.0,
            width: 20.0,
            height: 20.0,
        };
        let scaled = scale_rect_about_center(child, capsule, STACK_CAPSULE_BLOOMED_SCALE);
        assert!((scaled.width - 18.4).abs() < 1e-5);
        assert!((scaled.height - 18.4).abs() < 1e-5);
        assert!(scaled.x > child.x);
        assert!(scaled.y > child.y);
        let capsule_cx = capsule.x + capsule.width * 0.5;
        let before_dx = child.x + child.width * 0.5 - capsule_cx;
        let after_dx = scaled.x + scaled.width * 0.5 - capsule_cx;
        assert!((after_dx - before_dx * STACK_CAPSULE_BLOOMED_SCALE).abs() < 1e-5);
    }

    #[test]
    fn stack_capsule_text_transform_scales_without_shrink_width_churn() {
        let capsule = Rect {
            x: 100.0,
            y: 200.0,
            width: 220.0,
            height: 52.0,
        };
        let base_scale = 1.5;
        let matrix =
            stack_capsule_bloom_text_transform(base_scale, capsule, STACK_CAPSULE_BLOOMED_SCALE)
                .expect("bloomed scale should need a transform");
        let direct =
            scale_about_rect_center_matrix(base_scale, capsule, STACK_CAPSULE_BLOOMED_SCALE);
        assert!((matrix.M11 - direct.M11).abs() < 1e-6);
        assert!((matrix.M22 - direct.M22).abs() < 1e-6);
        assert!((matrix.M31 - direct.M31).abs() < 1e-6);
        assert!((matrix.M32 - direct.M32).abs() < 1e-6);
        assert!((matrix.M11 - 1.38).abs() < 1e-6);
        assert!((matrix.M22 - 1.38).abs() < 1e-6);

        let origin_x = capsule.x + capsule.width * 0.5;
        let origin_y = capsule.y + capsule.height * 0.5;
        assert!((matrix.M31 - origin_x * 0.08 * base_scale).abs() < 1e-5);
        assert!((matrix.M32 - origin_y * 0.08 * base_scale).abs() < 1e-5);
        assert!(stack_capsule_bloom_text_transform(base_scale, capsule, 1.0).is_none());

        let unscaled_fit_width = 132.0;
        let unscaled_sig = title_shrink_signature(
            "Benchmark Zone",
            unscaled_fit_width,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
            0.0,
        );
        let scaled_sig = title_shrink_signature(
            "Benchmark Zone",
            unscaled_fit_width * STACK_CAPSULE_BLOOMED_SCALE,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
            zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
            0.0,
        );
        assert_ne!(
            unscaled_sig, scaled_sig,
            "changing fit width during bloom would churn the shrink cache"
        );
        assert_eq!(
            unscaled_sig,
            title_shrink_signature(
                "Benchmark Zone",
                unscaled_fit_width,
                zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_PX,
                zone_pill_geometry::STACK_CAPSULE_TITLE_FONT_WEIGHT,
                0.0,
            ),
            "text transform keeps the title fit/cache width stable"
        );
    }

    #[test]
    fn stack_capsule_bloom_shadow_and_color_apply_css_opacity() {
        let target = stack_capsule_bloomed_target_shadow_stack();
        let bloomed =
            stack_capsule_bloom_shadow_stack(bento_nano_style::ShadowStack::NONE, 0.0, 1.0);
        assert_eq!(bloomed, target);
        assert!((bloomed.inner().offset_y - 14.0).abs() < f32::EPSILON);
        assert!((bloomed.inner().blur - 36.0).abs() < f32::EPSILON);
        assert!((bloomed.inner().color.a - 0.22).abs() < f32::EPSILON);
        assert!((bloomed.outer().color.a - 0.06).abs() < f32::EPSILON);

        let faded = fade_color(
            Color::rgba(1.0, 1.0, 1.0, 0.18),
            STACK_CAPSULE_BLOOMED_OPACITY,
        );
        assert!((faded.a - 0.099).abs() < 1e-6);
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

        let rect = drop_preview_rect_for_zone(&zone, Some(drag), false, 0.0, 0.0).expect("preview");

        // P3.8 paint-hit parity: drag-preview placement uses the same grid SSoTs
        // as painted cards. For a 240px zone, the 64-DIP readable-card floor
        // reflows the requested 4 columns into 3 effective columns:
        // cell_w = (240 - 16*2 - 8*2) / 3 = 64; col stride = 72.
        // last_x=130 lands in col 1, last_y=116 lands in row 0 because row 0
        // starts at zone_top(20) + ITEM_GRID_TOP_OFFSET_PX(56) = 76.
        assert!((rect.x - 98.0).abs() < 0.01);
        assert!((rect.y - 76.0).abs() < 0.01);
        assert!((rect.width - 64.0).abs() < 0.01);
        assert!((rect.height - item_grid::ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
    }

    #[test]
    fn drop_preview_targets_occupied_non_source_cell() {
        let mut zone = Zone::new(ZoneId(7), Cow::Borrowed("z"), 10, 20, 240, 180);
        zone.items.push(ZoneItem::new(
            ZoneItemId(8),
            "C:/Users/HP/Desktop/source-neighbor.lnk",
            "",
            0,
            0,
        ));
        zone.items.push(ZoneItem::new(
            ZoneItemId(9),
            "C:/Users/HP/Desktop/target.lnk",
            "",
            0,
            0,
        ));
        let drag = ActiveItemDragVisual {
            zone_id: ZoneId(8),
            item_id: ZoneItemId(1),
            last_x: 130.0,
            last_y: 116.0,
        };

        let preview =
            drop_preview_rect_for_zone(&zone, Some(drag), false, 0.0, 0.0).expect("preview");
        let resident_card = item_card_rect_for_item(&zone, &zone.items[1]);

        assert_eq!(preview, resident_card);
        assert_ne!(drag.zone_id, zone.id);
        assert_ne!(drag.item_id, zone.items[1].id);
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

#[cfg(test)]
mod zone_drag_merge_visual_tests {
    use super::{
        ZONE_DRAG_VISUAL_OPACITY, moved_zone_drag_source, zone_drag_visual_opacity, zone_draw_layer,
    };
    use crate::AppState;
    use bento_nano_style::Size;
    use bento_nano_zone::{Zone, ZoneId};

    fn app_with_source_and_target(source_x: i32, source_y: i32) -> AppState {
        let mut app = AppState::new();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(1), "Source", source_x, source_y, 160, 120));
        app.zones
            .add(Zone::new(ZoneId(2), "Target", 240, 80, 160, 120));
        app
    }

    #[test]
    fn zone_drag_visual_stays_opaque_without_drag() {
        let app = app_with_source_and_target(240, 80);
        let source = app.zones.get(ZoneId(1)).expect("source fixture");

        assert!(!moved_zone_drag_source(&app, ZoneId(1)));
        assert_eq!(zone_drag_visual_opacity(&app, ZoneId(1)), 1.0);
        assert_eq!(zone_draw_layer(&app, source), 0);
    }

    #[test]
    fn zone_drag_visual_stays_idle_before_drag_threshold_latches() {
        let app = app_with_source_and_target(240, 80);
        app.zone_drag.set(Some((ZoneId(1), 0, 0)));
        app.zone_drag_origin.set(Some((10, 10, false)));
        let source = app.zones.get(ZoneId(1)).expect("source fixture");

        assert!(!moved_zone_drag_source(&app, ZoneId(1)));
        assert_eq!(zone_drag_visual_opacity(&app, ZoneId(1)), 1.0);
        assert_eq!(zone_draw_layer(&app, source), 0);
    }

    #[test]
    fn moved_zone_uses_tauri_drag_opacity_even_without_merge_target() {
        let app = app_with_source_and_target(20, 20);
        app.zone_drag.set(Some((ZoneId(1), 0, 0)));
        app.zone_drag_origin.set(Some((10, 10, true)));
        let source = app.zones.get(ZoneId(1)).expect("source fixture");

        assert!(moved_zone_drag_source(&app, ZoneId(1)));
        assert_eq!(
            zone_drag_visual_opacity(&app, ZoneId(1)),
            ZONE_DRAG_VISUAL_OPACITY
        );
        assert_eq!(zone_draw_layer(&app, source), 2);
    }

    #[test]
    fn moved_source_stays_above_target_until_mouse_up_scores_the_merge() {
        let app = app_with_source_and_target(250, 90);
        app.zone_drag.set(Some((ZoneId(1), 0, 0)));
        app.zone_drag_origin.set(Some((10, 10, true)));
        let source = app.zones.get(ZoneId(1)).expect("source fixture");
        let target = app.zones.get(ZoneId(2)).expect("target fixture");

        assert_eq!(zone_draw_layer(&app, source), 2);
        assert_eq!(zone_draw_layer(&app, target), 0);
        assert_eq!(
            zone_drag_visual_opacity(&app, ZoneId(1)),
            ZONE_DRAG_VISUAL_OPACITY
        );
        assert_eq!(zone_drag_visual_opacity(&app, ZoneId(2)), 1.0);
    }
}

#[cfg(test)]
mod p0_click_through_region_tests {
    //! P0 desktop click-through (CLICKTHROUGH-FIX-VALIDATED.md) — pure-CPU
    //! geometry tests for [`chrome_region_rects`]. The GDI `SetWindowRgn`
    //! application is not headless-testable (needs a live HWND), same exemption
    //! as the GPU/window draw paths; these tests pin the DIP rect set the region
    //! is built from. No GPU / window / Argon2 → runs under the min-RSS suite.
    use super::{
        CHROME_REGION_SHADOW_MARGIN_DIP, chrome_region_rects, full_client_device_region,
        main_region_precedes_present,
    };
    use crate::AppState;
    use crate::business::{icons::IconKind, popover};
    use crate::state::ZoneDisplayMode;
    use bento_nano_platform::WindowKind;
    use bento_nano_style::{Rect, Size};
    use bento_nano_zone::{Zone, ZoneId};

    fn covered(rects: &[Rect], x: f32, y: f32) -> bool {
        rects
            .iter()
            .any(|r| x >= r.x && x < r.right() && y >= r.y && y < r.bottom())
    }

    fn pill_zone(id: u64, x: i32, y: i32) -> Zone {
        Zone::new(ZoneId(id), "Docs", x, y, 160, 120)
    }

    fn app_with_viewport() -> AppState {
        let mut app = AppState::new();
        app.viewport = Size {
            width: 1920.0,
            height: 1080.0,
        };
        // Hover is the default mode — a fresh zone is therefore a collapsed
        // pill (no hover / selection set), exercising the pill case by default.
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        app
    }

    #[test]
    fn active_drag_region_is_one_stable_full_client_rect() {
        let viewport = Size {
            width: 800.0,
            height: 600.0,
        };
        assert_eq!(
            full_client_device_region(viewport, 1.5),
            Some((0, 0, 1200, 900))
        );
        assert_eq!(
            full_client_device_region(
                Size {
                    width: 0.0,
                    height: 600.0,
                },
                1.0,
            ),
            None
        );
    }

    #[test]
    fn active_main_motion_installs_region_before_first_present() {
        assert!(main_region_precedes_present(WindowKind::Main, true, false));
        assert!(main_region_precedes_present(WindowKind::Main, false, true));
        assert!(!main_region_precedes_present(
            WindowKind::Main,
            false,
            false
        ));
        assert!(!main_region_precedes_present(
            WindowKind::Settings,
            true,
            true
        ));
    }

    #[test]
    fn blank_state_with_no_zones_is_empty() {
        // No zones, nothing open, no drag → no painted surface → empty region
        // → the WHOLE desktop is click-through.
        let app = app_with_viewport();
        let rects = chrome_region_rects(&app);
        assert!(rects.is_empty(), "blank state must yield no chrome rects");
        // A representative blank coord is therefore not covered.
        assert!(!covered(&rects, 800.0, 500.0));
    }

    #[test]
    fn main_surface_context_menu_adds_only_its_compact_input_bounds() {
        let app = app_with_viewport();
        let mut rows = popover::ContextMenuRows::new();
        rows.push(popover::ContextMenuRow::command(1, "Edit", IconKind::Edit));
        let mut session = popover::ContextMenuSession::new(rows, popover::ContextMenuRows::new());
        session.set_origin(420.0, 260.0);
        let expected = popover::context_menu_bounds(&session);
        app.active_context_menu.borrow_mut().replace(session);

        let rects = chrome_region_rects(&app);
        assert_eq!(rects.as_slice(), &[expected]);
        assert!(covered(&rects, expected.x + 20.0, expected.y + 20.0));
        assert!(!covered(&rects, 40.0, 40.0));
    }

    #[test]
    fn one_collapsed_pill_yields_one_rect_about_pill_size_plus_margin() {
        let mut app = app_with_viewport();
        app.zones.add(pill_zone(1, 300, 200));
        let rects = chrome_region_rects(&app);
        assert_eq!(rects.len(), 1, "one collapsed pill → exactly one rect");

        // The rect is the pill geometry inflated by the shadow margin on each
        // side. Compare against the SSoT `pill_layout_for_zone`.
        let zone = app.zones.iter().next().expect("zone present");
        let pill = crate::zone_pill_geometry::pill_layout_for_zone(zone, zone.items.len()).rect;
        let m = CHROME_REGION_SHADOW_MARGIN_DIP;
        let got = rects[0];
        assert!((got.x - (pill.x - m)).abs() < 0.5, "x inflated by margin");
        assert!((got.y - (pill.y - m)).abs() < 0.5, "y inflated by margin");
        assert!(
            (got.width - (pill.width + m * 2.0)).abs() < 0.5,
            "width inflated by 2×margin"
        );
        assert!(
            (got.height - (pill.height + m * 2.0)).abs() < 0.5,
            "height inflated by 2×margin"
        );

        // A coord at the pill CENTRE is inside the region (interactive).
        let cx = pill.x + pill.width / 2.0;
        let cy = pill.y + pill.height / 2.0;
        assert!(covered(&rects, cx, cy), "pill centre must be in region");

        // A coord far from any chrome is NOT covered → reaches the desktop.
        assert!(
            !covered(&rects, 1700.0, 950.0),
            "blank far corner must be click-through"
        );
    }

    #[test]
    fn click_mode_selected_zone_yields_its_full_body_rect() {
        let mut app = app_with_viewport();
        app.zones.add(pill_zone(7, 400, 300));
        // Selection is the structural expansion producer only in Click mode.
        // The expanded (full x/y/w/h) body rect is then the painted surface.
        app.set_zone_display_mode(ZoneDisplayMode::Click);
        app.selected_zone.set(Some(ZoneId(7)));
        let rects = chrome_region_rects(&app);
        assert_eq!(rects.len(), 1, "one expanded zone → one rect");

        let m = CHROME_REGION_SHADOW_MARGIN_DIP;
        let got = rects[0];
        // Body rect is (400, 300, 160, 120) inflated by the margin.
        assert!((got.x - (400.0 - m)).abs() < 0.5);
        assert!((got.y - (300.0 - m)).abs() < 0.5);
        assert!((got.width - (160.0 + m * 2.0)).abs() < 0.5);
        assert!((got.height - (120.0 + m * 2.0)).abs() < 0.5);

        // A point inside the expanded body is interactive; a point outside the
        // (inflated) body is click-through.
        assert!(covered(&rects, 480.0, 360.0), "body interior in region");
        assert!(
            !covered(&rects, 800.0, 800.0),
            "point well outside the body is click-through"
        );
    }

    #[test]
    fn settings_aux_window_does_not_expand_main_region() {
        let app = app_with_viewport();
        app.settings_open.set(true);
        let rects = chrome_region_rects(&app);
        assert!(rects.is_empty());
        assert!(!covered(&rects, 5.0, 5.0));
        assert!(!covered(&rects, 1900.0, 1070.0));
    }

    #[test]
    fn blank_coord_between_two_pills_is_click_through() {
        let mut app = app_with_viewport();
        // Two well-separated collapsed pills with a wide blank gap between.
        app.zones.add(pill_zone(1, 100, 100));
        app.zones.add(pill_zone(2, 1000, 800));
        let rects = chrome_region_rects(&app);
        assert_eq!(rects.len(), 2, "two collapsed pills → two rects");

        // Each pill centre is interactive…
        let z1 = app.zones.get(ZoneId(1)).expect("z1");
        let p1 = crate::zone_pill_geometry::pill_layout_for_zone(z1, z1.items.len()).rect;
        assert!(covered(
            &rects,
            p1.x + p1.width / 2.0,
            p1.y + p1.height / 2.0
        ));
        // …and the empty space between the two pills is click-through.
        assert!(
            !covered(&rects, 600.0, 450.0),
            "gap between pills must reach the desktop"
        );
    }

    #[test]
    fn oversized_zone_chrome_is_clamped_to_viewport() {
        // ROOT-CAUSE-corrupt-zone-geometry.md belt-and-suspenders: even if a
        // zone is sized FAR beyond the viewport (the legacy
        // `w=170667 h=91200` corruption), every returned region rect must stay
        // within the viewport + shadow margin so the whole window can never
        // catch every click.
        let mut app = app_with_viewport();
        // Expanded body sized many times the 1920×1080 viewport.
        app.zones
            .add(Zone::new(ZoneId(9), "Huge", 0, 0, 170_667, 91_200));
        app.set_zone_display_mode(ZoneDisplayMode::Click);
        app.selected_zone.set(Some(ZoneId(9)));

        let rects = chrome_region_rects(&app);
        assert!(!rects.is_empty(), "expanded zone must paint a body rect");

        let vp = app.viewport;
        let m = CHROME_REGION_SHADOW_MARGIN_DIP;
        for r in rects.iter() {
            // After clamping-then-inflating, no rect may extend past the
            // viewport by more than a single shadow margin on any edge.
            assert!(r.x >= -m - 0.5, "left within margin: {r:?}");
            assert!(r.y >= -m - 0.5, "top within margin: {r:?}");
            assert!(
                r.right() <= vp.width + m + 0.5,
                "right within viewport+margin: {r:?}"
            );
            assert!(
                r.bottom() <= vp.height + m + 0.5,
                "bottom within viewport+margin: {r:?}"
            );
        }

        // The viewport interior (the body) is still interactive…
        assert!(covered(&rects, 960.0, 540.0), "body interior in region");
        // …but a point BEYOND the real screen (where the corrupt body would
        // otherwise have stretched) is NOT covered — the desktop is alive.
        assert!(
            !covered(&rects, 5000.0, 5000.0),
            "far-offscreen point must be click-through"
        );
    }

    #[test]
    fn zone_fully_offscreen_yields_no_region_rect() {
        // A zone whose body lies entirely past the viewport contributes nothing
        // to the region (its clamp-intersection is empty), so the area stays
        // click-through.
        let mut app = app_with_viewport();
        app.zones
            .add(Zone::new(ZoneId(3), "Gone", 5000, 5000, 160, 120));
        app.set_zone_display_mode(ZoneDisplayMode::Click);
        app.selected_zone.set(Some(ZoneId(3)));

        let rects = chrome_region_rects(&app);
        assert!(
            rects.is_empty(),
            "fully-offscreen zone must add no region rect, got {rects:?}"
        );
        assert!(!covered(&rects, 960.0, 540.0));
    }
}
