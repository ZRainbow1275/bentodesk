use super::*;

pub(super) const TEXT_FORMAT_CACHE_CAPACITY: usize = 8;
pub(super) const IMAGE_WIDGET_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Tauri `.stack-wrapper--bloomed .stack-capsule { transform: scale(0.92) }`.
pub(super) const STACK_CAPSULE_BLOOMED_SCALE: f32 = 0.92;
/// Tauri `.stack-wrapper--bloomed .stack-capsule { opacity: 0.55 }`.
pub(super) const STACK_CAPSULE_BLOOMED_OPACITY: f32 = 0.55;
/// Tauri bloomed capsule transition window:
/// `transform/opacity/box-shadow/border-color 180ms`.
pub(super) const STACK_CAPSULE_BLOOMED_RECEDES_MS: f32 = 180.0;
pub(super) const STACK_CAPSULE_EMERGE_START_SCALE: f32 = 0.96;
pub(super) const STACK_CAPSULE_EMERGE_OVERSHOOT_SCALE: f32 = 1.02;
pub(super) const STACK_CAPSULE_EMERGE_OVERSHOOT_AT: f32 = 0.60;
/// Tauri `.bento-zone--dragging` / `.stack-wrapper--dragging` keep the full
/// capsule geometry and fade the carried surface to 70% opacity.
pub(super) const ZONE_DRAG_VISUAL_OPACITY: f32 = 0.70;
/// DComp presents the state replacement immediately, whereas Chromium often
/// coalesces the unmount/mount into the first animated frame. Never present a
/// fully transparent native stack replacement: start at three 60 Hz frames of
/// the authored 240 ms keyframe while retaining the same curve and endpoint.
pub(super) const STACK_CAPSULE_EMERGE_MIN_PRESENTED_PROGRESS: f32 =
    48.0 / animator::STACK_EMERGE_DURATION_MS as f32;
/// Tauri Bloom petal hover/active transform and transition.
pub(super) const STACK_BLOOM_ACTIVE_SCALE: f32 = 1.05;
pub(super) const STACK_BLOOM_ACTIVE_TRANSITION_MS: u32 = 180;
/// Tauri active-petal halo breathes after a short settle delay.
pub(super) const STACK_BLOOM_ACTIVE_PULSE_DELAY_MS: u32 = 600;
pub(super) const STACK_BLOOM_ACTIVE_PULSE_PERIOD_MS: u32 = 1_500;
/// Tauri `.stack-capsule.is-locked { opacity: 0.9 }`.
pub(super) const STACK_CAPSULE_LOCKED_OPACITY: f32 = 0.9;
/// Tauri `.stack-wrapper--locked .stack-capsule__badge { background: rgba(245, 158, 11, 0.14) }`.
pub(super) const STACK_CAPSULE_LOCKED_BADGE_FILL: Color = Color::from_u8(0xF5, 0x9E, 0x0B, 0x24);
/// Tauri `.stack-wrapper--locked .stack-capsule__badge { color: #fcd34d }`.
pub(super) const STACK_CAPSULE_LOCKED_BADGE_TEXT: Color = Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF);
/// Tauri `.stack-capsule.has-preview` ring:
/// `0 0 0 1px rgba(59, 130, 246, 0.42)`.
pub(super) const STACK_CAPSULE_PREVIEW_RING: Color = Color::from_u8(0x3B, 0x82, 0xF6, 0x6B);
/// Tauri `.stack-capsule__preview-indicator { background: rgba(59, 130, 246, 0.14) }`.
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_FILL: Color =
    Color::from_u8(0x3B, 0x82, 0xF6, 0x24);
/// Tauri `.stack-capsule__preview-indicator { color: #93c5fd }`.
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_TEXT: Color =
    Color::from_u8(0x93, 0xC5, 0xFD, 0xFF);
/// Tauri `.stack-capsule__preview-indicator { font-size: 11px; font-weight: 600 }`.
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_FONT_PX: f32 = 11.0;
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_FONT_WEIGHT: u16 = 600;
/// Tauri `.stack-capsule__preview-indicator { padding: 3px 8px }`.
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_PAD_X: f32 = 8.0;
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_HEIGHT: f32 = 20.0;
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_MIN_WIDTH: f32 = 34.0;
pub(super) const STACK_CAPSULE_PREVIEW_INDICATOR_MAX_WIDTH: f32 = 82.0;
pub(super) type DeviceRegionRect = (i32, i32, i32, i32);

#[inline]
pub(super) fn full_client_device_region(
    viewport: bento_nano_style::Size,
    scale: f32,
) -> Option<DeviceRegionRect> {
    let scale = scale.max(0.01);
    let right = (viewport.width * scale).ceil() as i32;
    let bottom = (viewport.height * scale).ceil() as i32;
    (right > 0 && bottom > 0).then_some((0, 0, right, bottom))
}

#[inline]
pub(super) fn main_region_precedes_present(
    kind: WindowKind,
    zone_drag_active: bool,
    zone_resize_active: bool,
) -> bool {
    kind == WindowKind::Main && (zone_drag_active || zone_resize_active)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StackCapsuleBloomVisual {
    pub(super) recede_t: f32,
    pub(super) scale: f32,
    pub(super) opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StackCapsuleEmergeVisual {
    pub(super) scale: f32,
    pub(super) opacity: f32,
}

pub(super) struct CachedLinearGradientBrush {
    pub(super) top: Color,
    pub(super) bottom: Color,
    pub(super) brush: ID2D1LinearGradientBrush,
}

pub(super) fn direct_text_halign(align: dwrite::TextAlign) -> DWRITE_TEXT_ALIGNMENT {
    match align.h {
        dwrite::HAlign::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
        dwrite::HAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        dwrite::HAlign::Trailing => DWRITE_TEXT_ALIGNMENT_TRAILING,
    }
}

pub(super) fn direct_text_valign(align: dwrite::TextAlign) -> DWRITE_PARAGRAPH_ALIGNMENT {
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
pub(super) const PANEL_ACCENT_EDGE_THICKNESS_PX: f32 = 2.0;
// Tauri `ItemCard.css` uses `--font-size-xs` (11px).  The previous 14px
// runtime-frame override made short names dominate the grid while long names
// collapsed to the 8px floor, so one row visibly mixed several type scales.
pub(super) const ITEM_LABEL_BASE_FONT_PX: f32 = 11.0;
pub(super) const ITEM_LABEL_MIN_FONT_PX: f32 = 8.0;
pub(super) const ITEM_LABEL_BOTTOM_INSET_PX: f32 = 8.0;

#[inline]
pub(super) fn item_label_text_color_for_reference(
    pal: bento_nano_style::tokens::PaletteTauri,
) -> Color {
    pal.text_secondary
}

/// Frosted-backdrop rollback switch (`receipts/FROSTED-BACKDROP-SPEC.md` §
/// "Degrade ladder" #3, Wave G Mica-leak precedent). When `false` the entire
/// desktop capture + blur path is skipped — no `screencap` call, no bitmap
/// brush — and `fill_frosted_rect` collapses to a plain `fill_rounded_rect`
/// (the single flat tint, never the old double layer). Flip to `false` during
/// live verify if the real-acrylic frost misbehaves.
pub(super) const FROSTED_BACKDROP: bool = true;

/// Native auxiliary HWNDs cannot reuse Main's monitor-aligned wallpaper
/// capture without making the backdrop slide independently while the window is
/// dragged. Until Windows Acrylic is actually active for that HWND, keep the
/// card surface solid and leave transparency only in the rounded outer corners.
#[inline]
pub(super) fn opaque_auxiliary_surface(color: Color) -> Color {
    with_alpha(color, 1.0)
}

/// Flat fallback opacity when even the captured source bitmap is unavailable.
/// Keep enough density to mute Explorer labels without turning every Zone into
/// the opaque black block seen in the 2026-07-13 hand test.
pub(super) const FROSTED_FALLBACK_MIN_ALPHA: f32 = 0.78;

#[inline]
pub(super) fn frosted_fallback_underlay(tint: Color) -> Option<Color> {
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
pub(super) fn frosted_group_backdrop_opacity(tint_alpha: f32, group_opacity: f32) -> f32 {
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
pub(super) const FROSTED_BACKDROP_DOWNSAMPLE: u32 = 4;

/// Frosted-backdrop gaussian standard deviation in DOWNSAMPLED px. Tauri uses
/// separate blur tokens (`--blur-zen: blur(20px) saturate(160%)`,
/// `--blur-expanded: blur(24px) saturate(170%)`), but nano keeps one baked
/// backdrop bitmap to preserve the strict memory budget. Bias the shared bitmap
/// to the always-visible collapsed capsule token: at downsample 4 the source
/// stddev is `20 / 4 = 5.0` (Blink maps `blur(r)` to `feGaussianBlur
/// stdDeviation = r` in CSS px). Expanded panels still get their stronger
/// 82%-alpha tint on top of this same capture.
pub(super) const FROSTED_BACKDROP_STDDEV: f32 = 5.0;

/// Frosted-backdrop post-blur saturation factor (`D2D1Saturation` chained after
/// the gaussian) for Tauri dark `--blur-zen` `saturate(160%)`.
pub(super) const FROSTED_BACKDROP_SATURATION_DARK: f32 = 1.6;

/// Frosted-backdrop post-blur saturation factor for Tauri light `--blur-zen`
/// `saturate(130%)`. The shared nano backdrop is re-baked when theme polarity
/// changes; it is NOT a second long-lived bitmap, so the memory ceiling stays
/// tied to one cached capture.
pub(super) const FROSTED_BACKDROP_SATURATION_LIGHT: f32 = 1.3;
pub(super) const AUXILIARY_OPEN_ANIMATION_MS: u32 = 160;

#[inline]
pub(super) fn expanded_panel_accent_clip_rect(
    rect: bento_nano_style::Rect,
) -> bento_nano_style::Rect {
    bento_nano_style::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: PANEL_ACCENT_EDGE_THICKNESS_PX.min(rect.height.max(0.0)),
    }
}

#[inline]
pub(super) fn lerp_rect_clamped(
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
pub(super) fn expanded_header_title_rect(
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
pub(super) fn morph_zen_content_to_header(
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
pub(super) fn moved_zone_drag_source(app: &AppState, zone_id: ZoneId) -> bool {
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
pub(super) fn zone_drag_visual_opacity(app: &AppState, zone_id: ZoneId) -> f32 {
    if moved_zone_drag_source(app, zone_id) {
        ZONE_DRAG_VISUAL_OPACITY
    } else {
        1.0
    }
}

#[inline]
pub(super) fn zone_draw_layer(app: &AppState, zone: &Zone) -> u8 {
    if moved_zone_drag_source(app, zone.id) {
        2
    } else if app.zone_on_top(zone) {
        1
    } else {
        0
    }
}

#[inline]
pub(super) fn collapsed_pill_display_count(app: &AppState, zone: &Zone) -> usize {
    app.zones
        .stack_member_ids(zone.id)
        .map(|members| members.len())
        .unwrap_or_else(|| zone.items.len())
}

#[inline]
pub(super) fn tauri_zone_accent_color(zone_accent: Option<&str>) -> Option<Color> {
    zone_accent.and_then(parse_hex_color)
}

#[inline]
pub(super) fn tauri_badge_fill(zone_accent: Option<&str>, fallback_badge_bg: Color) -> Color {
    tauri_zone_accent_color(zone_accent).unwrap_or(fallback_badge_bg)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PanelHeaderButtonChrome {
    pub(super) background: Option<Color>,
    pub(super) glyph: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuxiliaryActionEmphasis {
    Primary,
    Secondary,
    Danger,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AuxiliaryActionChrome {
    pub(super) fill: Color,
    pub(super) border: Color,
    pub(super) text: Color,
}

#[inline]
pub(super) fn auxiliary_action_chrome(
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
pub(super) struct ExpandedPanelAuxChrome {
    pub(super) live_folder_fill: Color,
    pub(super) live_folder_text: Color,
}

#[inline]
pub(super) fn expanded_panel_aux_chrome(
    pal: bento_nano_style::tokens::PaletteTauri,
) -> ExpandedPanelAuxChrome {
    ExpandedPanelAuxChrome {
        live_folder_fill: with_alpha(pal.text_primary, 0.08),
        live_folder_text: pal.text_muted,
    }
}

#[inline]
pub(super) fn panel_header_button_chrome(
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
pub(super) struct SettingsThemeCardChrome {
    pub(super) fill: Color,
    pub(super) border: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StackCapsuleBadgeChrome {
    pub(super) fill: Color,
    pub(super) text: Color,
}

#[inline]
pub(super) fn settings_theme_card_chrome(
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
pub(super) fn stack_capsule_locked_opacity(is_locked: bool) -> f32 {
    if is_locked {
        STACK_CAPSULE_LOCKED_OPACITY
    } else {
        1.0
    }
}

#[inline]
pub(super) fn stack_capsule_badge_chrome(
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
pub(super) fn stack_capsule_is_locked(
    app: &AppState,
    anchor: &Zone,
    member_ids: &[ZoneId],
) -> bool {
    anchor.locked
        || member_ids.iter().any(|member_id| {
            app.zones
                .get(*member_id)
                .is_some_and(|member| member.locked)
        })
}

#[inline]
pub(super) fn stack_capsule_has_preview(app: &AppState, anchor_id: ZoneId) -> bool {
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
pub(super) fn stack_surface_allows_bloom(app: &AppState) -> bool {
    app.selected_zone.get().is_none()
        && app
            .stack_tray
            .borrow()
            .as_ref()
            .is_none_or(stack_tray::StackTrayState::is_bloom_preview)
}

#[inline]
pub(super) fn stack_capsule_show_preview_indicator(has_preview: bool, recede_t: f32) -> bool {
    has_preview && recede_t <= f32::EPSILON
}

#[inline]
pub(super) fn stack_capsule_preview_indicator_width(label: &str) -> f32 {
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
pub(super) fn item_label_visible_name(name: &str) -> &str {
    let Some(ext) = name.get(name.len().saturating_sub(4)..) else {
        return name;
    };
    if !(ext.eq_ignore_ascii_case(".lnk") || ext.eq_ignore_ascii_case(".url")) {
        return name;
    }
    name.get(..name.len() - 4).unwrap_or(name)
}

#[inline]
pub(super) fn item_label_font_size_for_width(text: &str, avail_w: f32) -> f32 {
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
pub(super) fn item_label_group_font_size<'a>(labels: impl Iterator<Item = (&'a str, f32)>) -> f32 {
    labels.fold(ITEM_LABEL_BASE_FONT_PX, |group_px, (text, avail_w)| {
        group_px.min(item_label_font_size_for_width(text, avail_w))
    })
}

#[inline]
pub(super) fn item_label_estimated_width(text: &str, font_px: f32) -> f32 {
    let mut ems = 0.0_f32;
    for ch in text.chars() {
        ems += item_label_char_width_em(ch);
    }
    ems * font_px
}

#[inline]
pub(super) fn item_label_char_width_em(ch: char) -> f32 {
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
pub(super) fn item_icon_slots_for_card(
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
pub(super) fn item_label_rect_for_card(
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
pub(super) struct ActiveItemDragVisual {
    pub(super) zone_id: ZoneId,
    pub(super) item_id: ZoneItemId,
    pub(super) last_x: f32,
    pub(super) last_y: f32,
}

#[derive(Clone)]
pub(super) struct CachedTextFormat {
    pub(super) family: SmolStr,
    pub(super) size_pt: f32,
    pub(super) weight: u16,
    pub(super) line_height: f32,
    pub(super) format: IDWriteTextFormat,
}
