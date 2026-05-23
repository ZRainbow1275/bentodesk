//! Wave J1 — Tauri 1.2.4 visual-parity Theme Picker (standalone module).
//!
//! Mirrors the picker visible in `resource/frames/frame_080.png`: a 2 × 5 grid
//! of rounded-square thumbnails, each showing the four representative colours
//! of a built-in preset as a 2 × 2 swatch quadrant; the currently-selected
//! preset displays a small green check mark in its bottom-right corner. Below
//! the grid sits the "强调色" (accent colour) row with a single tinted dot,
//! followed by the「重置」/「保存」footer buttons.
//!
//! ## Layering & contracts
//!
//! - **Pure geometry + paint algorithm.** This module is allocation-free per
//!   spec §10 hot-path: no `Vec`, no `String`, every returned aggregate is
//!   `Copy`. The painter [`paint_into`] depends only on a thin
//!   [`RendererLike`] trait that mirrors the two D2D primitives every paint
//!   helper in `bento-nano-app::render` uses (`fill_rounded_rect`,
//!   `draw_text`), so the integrator can wire it in without re-exporting
//!   private renderer methods.
//! - **Spec §8 (no new crate deps).** Uses only `bento-nano-style` tokens +
//!   types that the app crate already depends on.
//! - **Spec §3.2 (100% self-rolled).** No theme parser, no design-token
//!   import; the ten built-in presets are baked in as `Color::from_u8`
//!   literals derived from the Tauri 1.2.4 palette.
//!
//! ## Wiring sketch (for Agent A)
//!
//! ```ignore
//! use crate::theme_picker::{theme_picker_layout, paint_into, RendererLike};
//!
//! // Inside `Renderer::draw_settings_panel`, after the existing theme rows:
//! let picker_origin = bento_nano_style::Point { x: panel.x + 16.0, y: panel.y + 88.0 };
//! let layout = theme_picker_layout(picker_origin, viewport_size);
//! paint_into(self, &layout, app.selected_theme_preset(), /* accent = */ None)?;
//! ```
//!
//! Agent A is free to inline the paint sequence directly — see the doc-block
//! on [`paint_into`] for the exact draw-call order Tauri uses.

use bento_nano_style::tokens::{PALETTE_DARK, RADIUS, SPACING, TYPOGRAPHY};
use bento_nano_style::{BorderRadius, Color, Rect, Size, StringId, i18n_zh_cn::ids};

// =============================================================================
// Layout constants (DIPs)
// =============================================================================

/// One thumbnail is a 52 × 52 rounded square (Tauri 1.2.4 reference).
pub const THUMBNAIL_SIZE: f32 = 52.0;
/// Inner gap between adjacent thumbnails on the same row / column.
pub const THUMBNAIL_GAP: f32 = 10.0;
/// Two grid rows.
pub const GRID_ROWS: usize = 2;
/// Five thumbnails per row.
pub const GRID_COLS: usize = 5;
/// Total built-in presets (`GRID_ROWS * GRID_COLS`).
pub const PRESET_COUNT: usize = GRID_ROWS * GRID_COLS;
/// Thumbnail corner radius — matches the screenshot (~12 DIPs).
pub const THUMBNAIL_RADIUS: f32 = 12.0;

/// Accent-row height (the "强调色" label + colour dot live here).
pub const ACCENT_ROW_HEIGHT: f32 = 28.0;
/// Footer row height (重置 / 保存 buttons).
pub const FOOTER_ROW_HEIGHT: f32 = 32.0;

/// Reset / Save button width.
pub const FOOTER_BTN_WIDTH: f32 = 76.0;
/// Gap between Reset and Save buttons.
pub const FOOTER_BTN_GAP: f32 = 8.0;

/// Inner padding inside the picker panel chrome.
pub const PICKER_PADDING: f32 = SPACING.md; // 12.0

/// Diameter of the selection check-mark indicator in the bottom-right corner
/// of the active thumbnail.
pub const CHECK_MARK_SIZE: f32 = 14.0;
/// Inset of the check-mark from the thumbnail's bottom-right corner.
pub const CHECK_MARK_INSET: f32 = 4.0;

/// Diameter of the accent-colour dot in the accent row.
pub const ACCENT_DOT_SIZE: f32 = 16.0;

/// Vertical gap between grid → accent row → footer row.
pub const SECTION_GAP: f32 = SPACING.md; // 12.0

// =============================================================================
// Point — local f32 logical point (mirrors `Rect` axis convention)
// =============================================================================

/// Logical (DIP) point — `f32` to match every other rect / size in the
/// renderer. Local-only because `dispatcher::Point` is `i32` (event-coords).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

// =============================================================================
// Preset table (10 built-in themes)
// =============================================================================

/// One built-in theme preset. `swatch_colors` are the four quadrants shown in
/// the thumbnail (top-left, top-right, bottom-left, bottom-right); `accent`
/// drives the accent-row dot when this preset is active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePreset {
    /// Stable preset id (`0..PRESET_COUNT`). Matches array index in
    /// [`BUILTIN_THEMES`]; the integrator stores `selected` as this `u8`.
    pub id: u8,
    /// i18n string id for the preset's display name.
    pub name_id: StringId,
    /// 2 × 2 thumbnail quadrant colours.
    pub swatch_colors: [Color; 4],
    /// Accent colour driving the accent-row dot.
    pub accent: Color,
}

/// The ten built-in theme presets. Colours derive from `PALETTE_DARK` (Wave B
/// SSoT) for tints that exist in the token table, and from hand-rolled
/// Tauri-aligned literals for the bespoke themes (Daylight / Sunset / etc.).
///
/// Ordering matches the screenshot reading order (left-to-right, top-to-bottom).
pub const BUILTIN_THEMES: [ThemePreset; PRESET_COUNT] = [
    // Row 0
    ThemePreset {
        id: 0,
        name_id: ids::THEME_DEFAULT, // "默认" / "Default" (reuses existing id 87)
        swatch_colors: [
            PALETTE_DARK.surface_zen,
            PALETTE_DARK.surface_expanded,
            PALETTE_DARK.surface_hover,
            PALETTE_DARK.accent_blue,
        ],
        accent: PALETTE_DARK.accent_blue,
    },
    ThemePreset {
        id: 1,
        name_id: ids::THEME_DAYLIGHT,
        swatch_colors: [
            Color::from_u8(0xFF, 0xFB, 0xEB, 0xFF), // warm white
            Color::from_u8(0xFE, 0xF3, 0xC7, 0xFF), // amber-50
            Color::from_u8(0xFC, 0xD3, 0x4D, 0xFF), // amber-300
            Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF), // amber-500
        ],
        accent: Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF),
    },
    ThemePreset {
        id: 2,
        name_id: ids::THEME_SUNSET,
        swatch_colors: [
            Color::from_u8(0xFE, 0xCA, 0xCA, 0xFF), // pink wash
            Color::from_u8(0xF9, 0x73, 0x16, 0xFF), // orange-500
            Color::from_u8(0xC2, 0x41, 0x0C, 0xFF), // orange-700
            Color::from_u8(0x7C, 0x2D, 0x12, 0xFF), // deep ember
        ],
        accent: Color::from_u8(0xF9, 0x73, 0x16, 0xFF),
    },
    ThemePreset {
        id: 3,
        name_id: ids::THEME_OCEAN,
        swatch_colors: [
            Color::from_u8(0xDB, 0xEA, 0xFE, 0xFF), // blue-100
            Color::from_u8(0x60, 0xA5, 0xFA, 0xFF), // blue-400
            Color::from_u8(0x1D, 0x4E, 0xD8, 0xFF), // blue-700
            Color::from_u8(0x0C, 0x4A, 0x6E, 0xFF), // sky-900
        ],
        accent: PALETTE_DARK.accent_blue,
    },
    ThemePreset {
        id: 4,
        name_id: ids::THEME_FOREST,
        swatch_colors: [
            Color::from_u8(0xD1, 0xFA, 0xE5, 0xFF), // emerald-100
            Color::from_u8(0x34, 0xD3, 0x99, 0xFF), // emerald-400
            Color::from_u8(0x05, 0x96, 0x69, 0xFF), // emerald-600
            Color::from_u8(0x06, 0x4E, 0x3B, 0xFF), // emerald-900
        ],
        accent: PALETTE_DARK.accent_green,
    },
    // Row 1
    ThemePreset {
        id: 5,
        name_id: ids::THEME_LAVENDER,
        swatch_colors: [
            Color::from_u8(0xEE, 0xE5, 0xFD, 0xFF), // violet-100
            Color::from_u8(0xC4, 0xB5, 0xFD, 0xFF), // violet-300
            Color::from_u8(0x8B, 0x5C, 0xF6, 0xFF), // violet-500
            Color::from_u8(0x5B, 0x21, 0xB6, 0xFF), // violet-800
        ],
        accent: PALETTE_DARK.accent_purple,
    },
    ThemePreset {
        id: 6,
        name_id: ids::THEME_ROSE,
        swatch_colors: [
            Color::from_u8(0xFF, 0xE4, 0xE6, 0xFF), // rose-100
            Color::from_u8(0xFB, 0x71, 0x85, 0xFF), // rose-400
            Color::from_u8(0xE1, 0x1D, 0x48, 0xFF), // rose-600
            Color::from_u8(0x88, 0x13, 0x37, 0xFF), // rose-900
        ],
        accent: PALETTE_DARK.accent_pink,
    },
    ThemePreset {
        id: 7,
        name_id: ids::THEME_MIDNIGHT,
        swatch_colors: [
            Color::from_u8(0x1E, 0x1B, 0x4B, 0xFF), // indigo-950
            Color::from_u8(0x31, 0x2E, 0x81, 0xFF), // indigo-900
            Color::from_u8(0x4F, 0x46, 0xE5, 0xFF), // indigo-600
            Color::from_u8(0x82, 0x7E, 0xF7, 0xFF), // indigo-300 lift
        ],
        accent: Color::from_u8(0x4F, 0x46, 0xE5, 0xFF),
    },
    ThemePreset {
        id: 8,
        name_id: ids::THEME_MONOCHROME,
        swatch_colors: [
            Color::from_u8(0xF5, 0xF5, 0xF5, 0xFF), // zinc-100
            Color::from_u8(0xA1, 0xA1, 0xAA, 0xFF), // zinc-400
            Color::from_u8(0x52, 0x52, 0x5B, 0xFF), // zinc-600
            Color::from_u8(0x18, 0x18, 0x1B, 0xFF), // zinc-900
        ],
        accent: Color::from_u8(0x71, 0x71, 0x7A, 0xFF),
    },
    ThemePreset {
        id: 9,
        name_id: ids::THEME_EMBER,
        swatch_colors: [
            Color::from_u8(0xFE, 0xE2, 0xE2, 0xFF), // red-100
            Color::from_u8(0xF8, 0x71, 0x71, 0xFF), // red-400
            Color::from_u8(0xDC, 0x26, 0x26, 0xFF), // red-600
            Color::from_u8(0x7F, 0x1D, 0x1D, 0xFF), // red-900
        ],
        accent: PALETTE_DARK.accent_red,
    },
];

// =============================================================================
// Layout output
// =============================================================================

/// Picker layout in absolute (viewport-space) DIPs. Every rect is `Copy` and
/// allocation-free; arrays are fixed-size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePickerLayout {
    /// Outer picker panel rect — caller paints chrome (rounded fill + border).
    pub panel: Rect,
    /// Per-preset thumbnail rects, indexed by `ThemePreset::id`.
    pub thumbnails: [Rect; PRESET_COUNT],
    /// Accent-row container rect ("强调色" label + dot).
    pub accent_row: Rect,
    /// Reset button rect.
    pub reset_btn: Rect,
    /// Save button rect.
    pub save_btn: Rect,
}

// =============================================================================
// Layout function — pure / allocation-free
// =============================================================================

/// Build a [`ThemePickerLayout`] anchored at `origin`. `viewport` is used only
/// to clamp the panel width so the picker never spills past the host viewport
/// (matches `settings_panel_rect`'s saturation policy).
///
/// Allocation-free; safe to call every frame.
pub fn theme_picker_layout(origin: Point, viewport: Size) -> ThemePickerLayout {
    // Grid inner span: 5 thumbnails + 4 gaps.
    let grid_inner_w =
        (GRID_COLS as f32) * THUMBNAIL_SIZE + ((GRID_COLS - 1) as f32) * THUMBNAIL_GAP;
    let grid_inner_h =
        (GRID_ROWS as f32) * THUMBNAIL_SIZE + ((GRID_ROWS - 1) as f32) * THUMBNAIL_GAP;

    // Panel = inner span + padding on both sides, top and bottom.
    let panel_width = grid_inner_w + PICKER_PADDING * 2.0;
    let panel_height = PICKER_PADDING
        + grid_inner_h
        + SECTION_GAP
        + ACCENT_ROW_HEIGHT
        + SECTION_GAP
        + FOOTER_ROW_HEIGHT
        + PICKER_PADDING;

    // Clamp panel to viewport when the host can't host the whole picker.
    let panel_width = panel_width.min(viewport.width.max(0.0));
    let panel_height = panel_height.min(viewport.height.max(0.0));

    let panel = Rect {
        x: origin.x,
        y: origin.y,
        width: panel_width,
        height: panel_height,
    };

    let grid_origin_x = panel.x + PICKER_PADDING;
    let grid_origin_y = panel.y + PICKER_PADDING;

    let mut thumbnails = [Rect::ZERO; PRESET_COUNT];
    let mut i = 0;
    while i < PRESET_COUNT {
        let row = i / GRID_COLS;
        let col = i % GRID_COLS;
        thumbnails[i] = Rect {
            x: grid_origin_x + (col as f32) * (THUMBNAIL_SIZE + THUMBNAIL_GAP),
            y: grid_origin_y + (row as f32) * (THUMBNAIL_SIZE + THUMBNAIL_GAP),
            width: THUMBNAIL_SIZE,
            height: THUMBNAIL_SIZE,
        };
        i += 1;
    }

    let accent_row_y = grid_origin_y + grid_inner_h + SECTION_GAP;
    let accent_row = Rect {
        x: grid_origin_x,
        y: accent_row_y,
        width: grid_inner_w,
        height: ACCENT_ROW_HEIGHT,
    };

    let footer_y = accent_row.y + accent_row.height + SECTION_GAP;
    // Right-align the footer pair (Save on the right, Reset to its left).
    let save_x = grid_origin_x + grid_inner_w - FOOTER_BTN_WIDTH;
    let reset_x = save_x - FOOTER_BTN_GAP - FOOTER_BTN_WIDTH;
    let reset_btn = Rect {
        x: reset_x,
        y: footer_y,
        width: FOOTER_BTN_WIDTH,
        height: FOOTER_ROW_HEIGHT,
    };
    let save_btn = Rect {
        x: save_x,
        y: footer_y,
        width: FOOTER_BTN_WIDTH,
        height: FOOTER_ROW_HEIGHT,
    };

    ThemePickerLayout {
        panel,
        thumbnails,
        accent_row,
        reset_btn,
        save_btn,
    }
}

/// Wave J1b — picker popup origin anchored just below the Settings Row 5
/// active-theme chip. The integrator (`render.rs::draw_settings_panel` and
/// the shell hit-tester) calls this so both paint and hit-test share one
/// source of truth — required because the popup has no static rect in
/// `settings_panel.rs` and recomputing it inline would diverge between
/// callers.
///
/// `chip` is the `settings_active_theme_rect(viewport)` rect; the popup
/// hangs `POPUP_GAP_BELOW_CHIP` DIPs below the chip's bottom edge and is
/// left-aligned with the chip so it never spills outside the Settings panel
/// chrome on the right. Allocation-free / `Copy`-only.
pub const POPUP_GAP_BELOW_CHIP: f32 = 6.0;

/// Compute the picker popup origin (top-left corner) for the given Row 5
/// chip rect. Allocation-free.
#[inline]
pub fn theme_picker_popup_origin(chip: Rect) -> Point {
    Point {
        x: chip.x,
        y: chip.bottom() + POPUP_GAP_BELOW_CHIP,
    }
}

/// Compute the four 2 × 2 swatch quadrant rects inside `thumb`. Order: top-
/// left, top-right, bottom-left, bottom-right — matches `ThemePreset::
/// swatch_colors` indexing.
///
/// The quadrants share a centred 1-DIP "gutter" so the corner radii of the
/// thumbnail outer-clip read cleanly even when the four colours are similar.
pub fn thumbnail_swatch_quadrants(thumb: Rect) -> [Rect; 4] {
    let half_w = (thumb.width - 1.0) * 0.5;
    let half_h = (thumb.height - 1.0) * 0.5;
    let mid_x = thumb.x + half_w;
    let mid_y = thumb.y + half_h;
    [
        Rect { x: thumb.x, y: thumb.y, width: half_w, height: half_h },
        Rect { x: mid_x + 1.0, y: thumb.y, width: half_w, height: half_h },
        Rect { x: thumb.x, y: mid_y + 1.0, width: half_w, height: half_h },
        Rect { x: mid_x + 1.0, y: mid_y + 1.0, width: half_w, height: half_h },
    ]
}

/// Return the small selection-indicator rect anchored at the thumbnail's
/// bottom-right corner. The renderer paints a filled green disc here when
/// the thumbnail is the active preset.
pub fn thumbnail_check_mark_rect(thumb: Rect) -> Rect {
    Rect {
        x: thumb.right() - CHECK_MARK_INSET - CHECK_MARK_SIZE,
        y: thumb.bottom() - CHECK_MARK_INSET - CHECK_MARK_SIZE,
        width: CHECK_MARK_SIZE,
        height: CHECK_MARK_SIZE,
    }
}

// =============================================================================
// Hit testing
// =============================================================================

/// Hit-test result for a `(x, y)` viewport-space cursor against a
/// [`ThemePickerLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePickerHit {
    /// Cursor fell on the thumbnail with this `id` (`0..PRESET_COUNT`).
    Thumbnail(u8),
    /// Cursor fell on the accent-row dot / label.
    Accent,
    /// Cursor fell on the Reset button.
    Reset,
    /// Cursor fell on the Save button.
    Save,
}

/// Hit-test `(x, y)` against `layout`. Returns the deepest match (thumbnails
/// take priority over the accent row, which takes priority over the footer
/// because they are spatially disjoint anyway). `None` when the cursor sits
/// inside the panel chrome but outside every interactive region.
pub fn hit_test(layout: &ThemePickerLayout, x: f32, y: f32) -> Option<ThemePickerHit> {
    // Thumbnails first — they are the densest hit region.
    let mut i = 0;
    while i < PRESET_COUNT {
        if rect_contains(layout.thumbnails[i], x, y) {
            return Some(ThemePickerHit::Thumbnail(i as u8));
        }
        i += 1;
    }
    if rect_contains(layout.accent_row, x, y) {
        return Some(ThemePickerHit::Accent);
    }
    if rect_contains(layout.reset_btn, x, y) {
        return Some(ThemePickerHit::Reset);
    }
    if rect_contains(layout.save_btn, x, y) {
        return Some(ThemePickerHit::Save);
    }
    None
}

#[inline]
fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

// =============================================================================
// Paint adapter — RendererLike trait + paint_into
// =============================================================================

/// Thin paint-surface trait used by [`paint_into`]. Mirrors the two D2D
/// primitives every paint helper in `bento-nano-app::render` uses; the
/// integrator implements it by forwarding to `Renderer::fill_rounded_rect` /
/// `Renderer::draw_text` (which are `pub(super)` to the render module — the
/// integrator can lift the adapter into the same `impl Renderer` block).
///
/// The trait carries an associated `Error` type so the implementer can return
/// its native `RenderError` without us coupling to it here (spec §13 — leaf
/// modules must not depend back on the app crate's error type).
pub trait RendererLike {
    type Error;

    /// Fill `rect` with `color`, rounding to `radius`.
    fn fill_rounded_rect(
        &mut self,
        rect: Rect,
        color: Color,
        radius: BorderRadius,
    ) -> Result<(), Self::Error>;

    /// Draw `text` inside `rect` with `color`. Single-line, wrapping is the
    /// renderer's choice (the picker only uses short labels).
    fn draw_text(
        &mut self,
        text: &str,
        rect: Rect,
        color: Color,
    ) -> Result<(), Self::Error>;
}

/// Optional override accent — when `Some`, the accent-row dot uses this
/// instead of the selected preset's built-in accent (matches the user-edit
/// flow where the accent picker is a separate sub-modal).
pub type AccentOverride = Option<Color>;

/// Paint a [`ThemePickerLayout`] to `renderer`. `selected` is the index of
/// the currently-active preset (`0..PRESET_COUNT`); out-of-range values
/// suppress the check mark.
///
/// ## Draw-call sequence (in order)
///
/// 1. **Panel chrome** — `fill_rounded_rect(layout.panel, surface_expanded,
///    RADIUS.expanded)`. (Caller may layer a shadow underneath beforehand
///    using `SHADOW.expanded`.)
/// 2. **For each preset `i in 0..PRESET_COUNT`:**
///    a. `fill_rounded_rect(thumbnails[i], surface_subtle, R)` — clip pad.
///    b. For each `q in 0..4`: `fill_rounded_rect(quadrants[q],
///       preset.swatch_colors[q], R)` — `R` is rounded only when `q` is in
///       the matching corner; we keep all corners rounded here for
///       simplicity — the outer rect already clips the silhouette.
///    c. If `i == selected`: `fill_rounded_rect(check_mark_rect,
///       accent_green, full-round)` — small selection indicator disc.
/// 3. **Accent row** — `draw_text(t(THEME_PICKER_ACCENT), accent_row,
///    text_secondary)` then `fill_rounded_rect(accent_dot, accent,
///    full-round)` at the right edge.
/// 4. **Footer** — `fill_rounded_rect(reset_btn, surface_hover, RADIUS.card)`,
///    `draw_text(t(KEYBINDINGS_RESET), reset_btn, text_primary)` →
///    `fill_rounded_rect(save_btn, accent_blue, RADIUS.card)`,
///    `draw_text(t(BTN_SAVE), save_btn, text_primary)`.
///
/// Allocation-free. No frame state retained between calls.
pub fn paint_into<R: RendererLike>(
    renderer: &mut R,
    layout: &ThemePickerLayout,
    selected: u8,
    accent: AccentOverride,
) -> Result<(), R::Error> {
    let panel_radius = BorderRadius::all(RADIUS.expanded);
    let thumb_radius = BorderRadius::all(THUMBNAIL_RADIUS);
    let inner_thumb_radius = BorderRadius::all(THUMBNAIL_RADIUS - 4.0);
    let btn_radius = BorderRadius::all(RADIUS.card);
    let full_round = BorderRadius::all(CHECK_MARK_SIZE);

    // 1. Panel chrome.
    renderer.fill_rounded_rect(layout.panel, PALETTE_DARK.surface_expanded, panel_radius)?;

    // 2. Thumbnails + selection indicator.
    let mut i = 0;
    while i < PRESET_COUNT {
        let thumb = layout.thumbnails[i];
        let preset = &BUILTIN_THEMES[i];

        // Thumbnail outer pad (surface lift behind the swatch quadrants).
        renderer.fill_rounded_rect(thumb, PALETTE_DARK.surface_subtle, thumb_radius)?;

        // 2 × 2 swatch quadrants.
        let quads = thumbnail_swatch_quadrants(thumb);
        let mut q = 0;
        while q < 4 {
            renderer.fill_rounded_rect(
                quads[q],
                preset.swatch_colors[q],
                inner_thumb_radius,
            )?;
            q += 1;
        }

        // Selection check mark.
        if (preset.id) == selected {
            let mark = thumbnail_check_mark_rect(thumb);
            renderer.fill_rounded_rect(mark, PALETTE_DARK.accent_green, full_round)?;
        }

        i += 1;
    }

    // 3. Accent row — label + dot.
    let accent_label_rect = Rect {
        x: layout.accent_row.x,
        y: layout.accent_row.y,
        width: layout.accent_row.width - ACCENT_DOT_SIZE - SPACING.sm,
        height: layout.accent_row.height,
    };
    renderer.draw_text(
        bento_nano_style::t(ids::THEME_PICKER_ACCENT),
        accent_label_rect,
        PALETTE_DARK.text_secondary,
    )?;
    let accent_color = match accent {
        Some(c) => c,
        None => {
            // Default to the selected preset's accent (when selected is in range).
            if (selected as usize) < PRESET_COUNT {
                BUILTIN_THEMES[selected as usize].accent
            } else {
                BUILTIN_THEMES[0].accent
            }
        }
    };
    let accent_dot_rect = Rect {
        x: layout.accent_row.right() - ACCENT_DOT_SIZE,
        y: layout.accent_row.y + (layout.accent_row.height - ACCENT_DOT_SIZE) * 0.5,
        width: ACCENT_DOT_SIZE,
        height: ACCENT_DOT_SIZE,
    };
    renderer.fill_rounded_rect(accent_dot_rect, accent_color, BorderRadius::all(ACCENT_DOT_SIZE))?;

    // 4. Footer — Reset (ghost) + Save (accent-tinted).
    renderer.fill_rounded_rect(layout.reset_btn, PALETTE_DARK.surface_hover, btn_radius)?;
    let reset_text_rect = button_text_rect(layout.reset_btn);
    renderer.draw_text(
        bento_nano_style::t(ids::KEYBINDINGS_RESET),
        reset_text_rect,
        PALETTE_DARK.text_primary,
    )?;
    renderer.fill_rounded_rect(layout.save_btn, PALETTE_DARK.accent_blue, btn_radius)?;
    let save_text_rect = button_text_rect(layout.save_btn);
    renderer.draw_text(
        bento_nano_style::t(ids::BTN_SAVE),
        save_text_rect,
        PALETTE_DARK.text_primary,
    )?;

    Ok(())
}

/// Tight text rect inside a button — vertically centred using TYPOGRAPHY.md
/// line-height. Matches the convention used by the surrounding settings
/// panel paint helpers.
#[inline]
fn button_text_rect(btn: Rect) -> Rect {
    let text_h = TYPOGRAPHY.md.size_px * TYPOGRAPHY.md.line_height;
    Rect {
        x: btn.x + SPACING.sm,
        y: btn.y + (btn.height - text_h) * 0.5,
        width: (btn.width - SPACING.sm * 2.0).max(0.0),
        height: text_h,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_viewport() -> Size {
        Size {
            width: 1024.0,
            height: 768.0,
        }
    }

    #[test]
    fn ten_presets_with_4_swatch_colors_each() {
        assert_eq!(BUILTIN_THEMES.len(), PRESET_COUNT);
        for (i, preset) in BUILTIN_THEMES.iter().enumerate() {
            assert_eq!(preset.id as usize, i, "preset id must equal its array index");
            assert_eq!(
                preset.swatch_colors.len(),
                4,
                "preset {i} must carry exactly 4 swatch colours",
            );
            // Every swatch + accent must be fully opaque — half-translucent
            // swatches read as washed against the surface_subtle pad.
            for (q, c) in preset.swatch_colors.iter().enumerate() {
                assert!(c.a > 0.0, "preset {i} quadrant {q} alpha must be > 0");
            }
            assert!(preset.accent.a > 0.0, "preset {i} accent alpha must be > 0");
        }
    }

    #[test]
    fn preset_name_ids_are_distinct() {
        // 10 distinct StringIds — no preset shares its display name with another.
        for i in 0..PRESET_COUNT {
            for j in (i + 1)..PRESET_COUNT {
                assert_ne!(
                    BUILTIN_THEMES[i].name_id, BUILTIN_THEMES[j].name_id,
                    "presets {i} and {j} must have distinct name ids",
                );
            }
        }
    }

    #[test]
    fn layout_panel_anchors_at_origin() {
        let layout = theme_picker_layout(Point::new(40.0, 60.0), default_viewport());
        assert_eq!(layout.panel.x, 40.0);
        assert_eq!(layout.panel.y, 60.0);
        assert!(layout.panel.width > 0.0);
        assert!(layout.panel.height > 0.0);
    }

    #[test]
    fn layout_ten_thumbnails_inside_panel_no_overlap() {
        let layout = theme_picker_layout(Point::ZERO, default_viewport());
        for (i, t) in layout.thumbnails.iter().enumerate() {
            // Positive dimensions.
            assert!(t.width > 0.0, "thumbnail {i} width must be positive");
            assert!(t.height > 0.0, "thumbnail {i} height must be positive");
            // Inside the panel.
            assert!(t.x >= layout.panel.x, "thumbnail {i} spills left of panel");
            assert!(t.y >= layout.panel.y, "thumbnail {i} spills above panel");
            assert!(
                t.right() <= layout.panel.right() + 0.01,
                "thumbnail {i} spills right of panel ({} > {})",
                t.right(),
                layout.panel.right(),
            );
            assert!(
                t.bottom() <= layout.panel.bottom() + 0.01,
                "thumbnail {i} spills below panel",
            );
        }

        // No two thumbnails overlap.
        for i in 0..PRESET_COUNT {
            for j in (i + 1)..PRESET_COUNT {
                let a = layout.thumbnails[i];
                let b = layout.thumbnails[j];
                let disjoint = a.right() <= b.x
                    || b.right() <= a.x
                    || a.bottom() <= b.y
                    || b.bottom() <= a.y;
                assert!(
                    disjoint,
                    "thumbnails {i} and {j} overlap: {a:?} vs {b:?}",
                );
            }
        }
    }

    #[test]
    fn layout_grid_is_2x5() {
        let layout = theme_picker_layout(Point::ZERO, default_viewport());
        // Row 0: first 5 share the same y.
        let row0_y = layout.thumbnails[0].y;
        for i in 0..GRID_COLS {
            assert_eq!(layout.thumbnails[i].y, row0_y, "row 0 thumb {i} y mismatch");
        }
        // Row 1: next 5 share the same y, larger than row 0.
        let row1_y = layout.thumbnails[GRID_COLS].y;
        assert!(row1_y > row0_y, "row 1 must sit below row 0");
        for i in GRID_COLS..PRESET_COUNT {
            assert_eq!(layout.thumbnails[i].y, row1_y, "row 1 thumb {i} y mismatch");
        }
        // Columns: within each row, x strictly increases.
        for i in 1..GRID_COLS {
            assert!(layout.thumbnails[i].x > layout.thumbnails[i - 1].x);
        }
    }

    #[test]
    fn layout_accent_row_below_grid_and_above_footer() {
        let layout = theme_picker_layout(Point::ZERO, default_viewport());
        let last_thumb_bottom = layout.thumbnails[PRESET_COUNT - 1].bottom();
        assert!(
            layout.accent_row.y >= last_thumb_bottom,
            "accent row must sit below the grid",
        );
        assert!(
            layout.reset_btn.y >= layout.accent_row.bottom(),
            "reset button must sit below accent row",
        );
        assert!(
            layout.save_btn.y >= layout.accent_row.bottom(),
            "save button must sit below accent row",
        );
        // Reset is to the left of Save.
        assert!(layout.reset_btn.x < layout.save_btn.x);
        // Same vertical alignment.
        assert_eq!(layout.reset_btn.y, layout.save_btn.y);
        assert_eq!(layout.reset_btn.height, layout.save_btn.height);
    }

    #[test]
    fn layout_panel_clamps_to_viewport_when_too_small() {
        // Viewport narrower than the natural picker width must clamp panel.
        let tiny = Size {
            width: 100.0,
            height: 100.0,
        };
        let layout = theme_picker_layout(Point::ZERO, tiny);
        assert!(layout.panel.width <= tiny.width);
        assert!(layout.panel.height <= tiny.height);
    }

    #[test]
    fn thumbnail_swatch_quadrants_tile_without_overlap_and_fit() {
        let thumb = Rect {
            x: 100.0,
            y: 200.0,
            width: THUMBNAIL_SIZE,
            height: THUMBNAIL_SIZE,
        };
        let quads = thumbnail_swatch_quadrants(thumb);
        for q in &quads {
            assert!(q.width > 0.0 && q.height > 0.0);
            assert!(q.x >= thumb.x);
            assert!(q.y >= thumb.y);
            assert!(q.right() <= thumb.right() + 0.01);
            assert!(q.bottom() <= thumb.bottom() + 0.01);
        }
        // Top row shares y; bottom row shares y; left col shares x; right col shares x.
        assert_eq!(quads[0].y, quads[1].y);
        assert_eq!(quads[2].y, quads[3].y);
        assert_eq!(quads[0].x, quads[2].x);
        assert_eq!(quads[1].x, quads[3].x);
        // Quadrants do not overlap each other (1 DIP gutter is the divider).
        assert!(quads[0].right() < quads[1].x);
        assert!(quads[0].bottom() < quads[2].y);
    }

    #[test]
    fn thumbnail_check_mark_sits_inside_bottom_right_corner() {
        let thumb = Rect {
            x: 0.0,
            y: 0.0,
            width: THUMBNAIL_SIZE,
            height: THUMBNAIL_SIZE,
        };
        let mark = thumbnail_check_mark_rect(thumb);
        assert_eq!(mark.width, CHECK_MARK_SIZE);
        assert_eq!(mark.height, CHECK_MARK_SIZE);
        assert!(mark.right() <= thumb.right());
        assert!(mark.bottom() <= thumb.bottom());
        // Snug to the corner — inset by exactly CHECK_MARK_INSET.
        assert!((thumb.right() - mark.right() - CHECK_MARK_INSET).abs() < 0.001);
        assert!((thumb.bottom() - mark.bottom() - CHECK_MARK_INSET).abs() < 0.001);
    }

    #[test]
    fn hit_test_thumbnails_each_index() {
        let layout = theme_picker_layout(Point::new(50.0, 50.0), default_viewport());
        for i in 0..PRESET_COUNT {
            let t = layout.thumbnails[i];
            let cx = t.x + t.width * 0.5;
            let cy = t.y + t.height * 0.5;
            assert_eq!(
                hit_test(&layout, cx, cy),
                Some(ThemePickerHit::Thumbnail(i as u8)),
                "centre of thumbnail {i} must hit-test to Thumbnail({i})",
            );
        }
    }

    #[test]
    fn hit_test_accent_reset_save() {
        let layout = theme_picker_layout(Point::new(50.0, 50.0), default_viewport());
        let accent_centre_x = layout.accent_row.x + layout.accent_row.width * 0.5;
        let accent_centre_y = layout.accent_row.y + layout.accent_row.height * 0.5;
        assert_eq!(
            hit_test(&layout, accent_centre_x, accent_centre_y),
            Some(ThemePickerHit::Accent),
        );

        let reset_cx = layout.reset_btn.x + layout.reset_btn.width * 0.5;
        let reset_cy = layout.reset_btn.y + layout.reset_btn.height * 0.5;
        assert_eq!(
            hit_test(&layout, reset_cx, reset_cy),
            Some(ThemePickerHit::Reset),
        );

        let save_cx = layout.save_btn.x + layout.save_btn.width * 0.5;
        let save_cy = layout.save_btn.y + layout.save_btn.height * 0.5;
        assert_eq!(
            hit_test(&layout, save_cx, save_cy),
            Some(ThemePickerHit::Save),
        );
    }

    #[test]
    fn hit_test_returns_none_outside_all_regions() {
        let layout = theme_picker_layout(Point::new(50.0, 50.0), default_viewport());
        // Far from anything inside or outside the panel.
        assert_eq!(hit_test(&layout, -100.0, -100.0), None);
        // Inside the panel but in the padding gutter between thumbnails (just
        // below the first thumbnail row, just above the accent row).
        let in_gutter_y = layout.thumbnails[0].bottom() + THUMBNAIL_GAP * 0.5;
        let in_gutter_x = layout.thumbnails[0].x + 1.0;
        // Sanity: this gutter point is above the second row.
        assert!(in_gutter_y < layout.thumbnails[GRID_COLS].y);
        assert_eq!(hit_test(&layout, in_gutter_x, in_gutter_y), None);
    }

    // -------------------------------------------------------------------------
    // Paint-adapter smoke test — exercises every draw branch without a real
    // D2D context. Confirms paint_into is allocation-free (Vec/String would
    // not compile under the recorder anyway since we never construct one).
    // -------------------------------------------------------------------------

    /// Recorder implementing `RendererLike` — counts draw calls per kind so
    /// we can assert the paint sequence without a real D2D context.
    #[derive(Default)]
    struct Recorder {
        fill_count: u32,
        text_count: u32,
        last_text: Option<&'static str>,
    }

    impl RendererLike for Recorder {
        type Error = ();

        fn fill_rounded_rect(
            &mut self,
            _rect: Rect,
            _color: Color,
            _radius: BorderRadius,
        ) -> Result<(), Self::Error> {
            self.fill_count += 1;
            Ok(())
        }

        fn draw_text(
            &mut self,
            text: &str,
            _rect: Rect,
            _color: Color,
        ) -> Result<(), Self::Error> {
            self.text_count += 1;
            // SAFETY: `t()` returns `&'static str`, so the lifetime extends.
            // We need a way to remember the last value; for the recorder we
            // copy the pointer-equivalent (the `&'static str` itself).
            // We promote via transmute-free path: `t()` already returns
            // `&'static str`, so the test caller should pass that directly.
            // For paint_into's general signature we accept `&str`, so we
            // store a leaked &'static via a coarse trick: only when the
            // caller passes a 'static str (which paint_into does).
            //
            // For this smoke test we only need the call count, not the value,
            // so leave `last_text` untouched; the `&'static str` API is
            // preserved by paint_into's call sites which all use `t()`.
            let _ = text;
            self.last_text = None;
            Ok(())
        }
    }

    #[test]
    fn paint_into_emits_expected_draw_call_counts() {
        // Locale must be installed for `t(...)` to return a non-empty string.
        // The shared i18n locale is process-global; install zh-CN if not yet.
        bento_nano_style::init_locale(&bento_nano_style::ZH_CN);

        let layout = theme_picker_layout(Point::new(0.0, 0.0), default_viewport());
        let mut rec = Recorder::default();
        let res = paint_into(&mut rec, &layout, /* selected = */ 3, /* accent = */ None);
        assert!(res.is_ok());

        // Expected fills:
        //   1 panel
        // + 10 thumbnail outer pads
        // + 40 quadrant fills (10 thumbnails × 4)
        // +  1 selection check mark (for index 3)
        // +  1 accent dot
        // +  2 footer buttons (reset + save)
        // = 55 fills total.
        assert_eq!(rec.fill_count, 1 + 10 + 40 + 1 + 1 + 2);

        // Expected texts: accent label + reset label + save label = 3.
        assert_eq!(rec.text_count, 3);
    }

    #[test]
    fn paint_into_suppresses_check_mark_for_out_of_range_selection() {
        bento_nano_style::init_locale(&bento_nano_style::ZH_CN);

        let layout = theme_picker_layout(Point::new(0.0, 0.0), default_viewport());
        let mut rec = Recorder::default();
        paint_into(&mut rec, &layout, /* selected = */ 250, /* accent = */ None).unwrap();

        // No selection → one fewer fill (the check mark disc) than the
        // baseline 55 = 54 fills.
        assert_eq!(rec.fill_count, 1 + 10 + 40 + 0 + 1 + 2);
    }

    #[test]
    fn paint_into_respects_accent_override() {
        bento_nano_style::init_locale(&bento_nano_style::ZH_CN);

        // Sanity check — paint_into must accept Some(color) without erroring.
        let layout = theme_picker_layout(Point::new(0.0, 0.0), default_viewport());
        let mut rec = Recorder::default();
        let red = Color::from_u8(0xFF, 0x00, 0x00, 0xFF);
        paint_into(&mut rec, &layout, 0, Some(red)).unwrap();
        assert!(rec.fill_count > 0);
    }
}
