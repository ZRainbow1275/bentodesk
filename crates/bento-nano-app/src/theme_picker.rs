//! M6-UI — §3 Appearance inline theme grid (Tauri 1.2.4 visual parity).
//!
//! Replaces the Wave J1 popup picker (10 bespoke presets in a floating 2×5
//! grid with a Reset/Save footer + display-only accent dot) with the inline
//! §3 Appearance section Tauri 1.2.4 ships (`SettingsPanel.tsx:396-536`): the
//! **17** built-in themes laid out in a **4-column** grid, **grouped into 4
//! families** (Rounded Glass 9 · Solid 1 · Angular Modern 4 · Personality 3),
//! each card a 2×2 preview swatch + a centred name label, the active card
//! drawn with a 2-DIP accent-blue border + 10 %-blue fill tint. Below the
//! grid sits Tauri's compact **accent-colour** control.
//!
//! ## Layering & contracts
//!
//! - **Pure geometry + data.** This module is allocation-free per spec §10:
//!   no `Vec`, no `String`, every aggregate is `Copy`. [`AppearanceLayout`] is
//!   a fixed-cap `[Rect; N]` struct; the layout fn walks the preset table and
//!   computes every rect inline (no per-group `Vec`). The renderer
//!   (`render.rs::draw_settings_panel`) owns the paint so it can read the live
//!   `app.active_theme_tauri()` palette + reuse `fill_rounded_rect` /
//!   `stroke_rounded_rect` / `draw_text` directly.
//! - **Spec §8 (no new crate deps).** Uses only `bento-nano-style` tokens +
//!   types the app crate already depends on. The 17 preview-colour literals
//!   are baked `Color::from_u8`, honouring alpha for the two translucent
//!   `frosted` quadrants.
//! - **Spec §3.2 (100 % self-rolled).** No theme-JSON parser; the swatch
//!   colours are hand-transcribed from the Tauri `presets.ts` `preview_colors`.

use bento_nano_style::tokens::SPACING;
use bento_nano_style::{Color, Rect, StringId, i18n_zh_cn::ids};

// =============================================================================
// Inline layout constants (DIPs) — transcribed from Tauri CSS (§2).
// =============================================================================

/// `.theme-grid { grid-template-columns: repeat(4, 1fr) }`.
pub const THEME_GRID_COLS: usize = 4;
/// `.theme-grid { gap: 10px }` (both axes).
pub const THEME_GRID_GAP: f32 = 10.0;
/// `.theme-card { border-radius: 10px }`.
pub const THEME_CARD_RADIUS: f32 = 10.0;
/// `.theme-card { border: 2px solid … }`.
pub const THEME_CARD_BORDER: f32 = 2.0;
/// `.theme-card { padding: 10px 6px 8px }` — top.
pub const THEME_CARD_PAD_TOP: f32 = 10.0;
/// `.theme-card { padding: 10px 6px 8px }` — bottom.
pub const THEME_CARD_PAD_BOTTOM: f32 = 8.0;
/// `.theme-card { gap: 6px }` (swatch block → label).
pub const THEME_CARD_SWATCH_LABEL_GAP: f32 = 6.0;
/// `.theme-card__swatches { width/height: 40px }`.
pub const SWATCH_BLOCK_SIZE: f32 = 40.0;
/// `.theme-card__swatches { border-radius: 8px }`.
pub const SWATCH_BLOCK_RADIUS: f32 = 8.0;
/// `.theme-card__swatches { gap: 3px }` (inner quadrant gutter).
pub const SWATCH_INNER_GAP: f32 = 3.0;
/// `.theme-group__title { font-size: 10px }` CSS-derived line box.
pub const GROUP_HEADING_HEIGHT: f32 = 12.0;
/// `.theme-group { gap: 6px }` (heading → grid).
pub const GROUP_HEADING_TO_GRID_GAP: f32 = 6.0;
/// `.theme-card__label { font-size: 10px; line-height: 1.2 }`.
pub const CARD_LABEL_HEIGHT: f32 = 12.0;
/// `.theme-groups { gap: var(--spacing-md, 12px) }`.
pub const GROUP_TO_GROUP_GAP: f32 = 12.0;

/// One card's border-box height.
///
/// Tauri `.theme-card` uses CSS' default `box-sizing: content-box`: the
/// 2px border sits outside `padding + swatch + gap + label`. N137 runtime
/// PrintWindow measured the active card at 152×120 px on the 1.5× display
/// (80 CSS px high), so include both vertical borders in the layout height.
pub const THEME_CARD_HEIGHT: f32 = THEME_CARD_PAD_TOP
    + SWATCH_BLOCK_SIZE
    + THEME_CARD_SWATCH_LABEL_GAP
    + CARD_LABEL_HEIGHT
    + THEME_CARD_PAD_BOTTOM
    + THEME_CARD_BORDER * 2.0;

/// Accent row — Tauri `.settings-row { min-height: 42px }`.
pub const ACCENT_ROW_HEIGHT: f32 = 42.0;
/// Inline hex editor width for `#rrggbb`.
pub const ACCENT_INPUT_W: f32 = 84.0;
/// Inline hex editor height.
pub const ACCENT_INPUT_H: f32 = 28.0;
/// Gap between the row label and the inline hex editor.
pub const ACCENT_INPUT_GAP: f32 = 8.0;
/// Tauri `.settings-row__color { width: 36px }`.
pub const ACCENT_PICKER_W: f32 = 36.0;
/// Native Windows colour-dialog button height.
pub const ACCENT_PICKER_H: f32 = ACCENT_INPUT_H;
/// Inline accent reset button width.
pub const ACCENT_CLEAR_W: f32 = 52.0;
/// Inline accent reset button height.
pub const ACCENT_CLEAR_H: f32 = ACCENT_INPUT_H;
/// Accent swatch dot diameter (≈16 DIP, gap 8).
pub const ACCENT_DOT_SIZE: f32 = 16.0;
/// Gap between adjacent accent swatch dots.
pub const ACCENT_DOT_GAP: f32 = 8.0;
/// Number of VIBRANT accent swatches (Control B MVP strip).
pub const ACCENT_SWATCH_COUNT: usize = 12;
/// Vertical gap between the theme grid and the accent row.
pub const ACCENT_ROW_TOP_GAP: f32 = SPACING.md; // 12.0

// =============================================================================
// Point — local f32 logical point.
// =============================================================================

/// Logical (DIP) point — `f32` to match every rect / size in the renderer.
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
// Theme family group.
// =============================================================================

/// One of the four Tauri theme families (`THEME_GROUP_ORDER`). Drives the
/// group heading painted above each family's 4-col grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeGroup {
    /// 圆角玻璃 / Rounded Glass (9 themes).
    Rounded,
    /// 实心 / Solid (1 theme).
    Solid,
    /// 方角现代 / Angular Modern (4 themes).
    Angular,
    /// 个性 / Personality (3 themes).
    Personality,
}

impl ThemeGroup {
    /// i18n string id for the group heading.
    pub const fn heading_id(self) -> StringId {
        match self {
            ThemeGroup::Rounded => ids::THEME_GROUP_ROUNDED,
            ThemeGroup::Solid => ids::THEME_GROUP_SOLID,
            ThemeGroup::Angular => ids::THEME_GROUP_ANGULAR,
            ThemeGroup::Personality => ids::THEME_GROUP_PERSONALITY,
        }
    }
}

/// Render order of the four families (Tauri `THEME_GROUP_ORDER`). The paint /
/// layout loops walk groups in this order; `ThemePreset::id` stays the flat
/// `BUILTIN_THEMES` array index (stable) regardless of visual order.
pub const THEME_GROUP_ORDER: [ThemeGroup; 4] = [
    ThemeGroup::Rounded,
    ThemeGroup::Solid,
    ThemeGroup::Angular,
    ThemeGroup::Personality,
];

// =============================================================================
// Preset table (17 built-in themes).
// =============================================================================

/// Total built-in theme presets (Tauri `BUILTIN_THEMES`).
pub const PRESET_COUNT: usize = 17;

/// One built-in theme preset. `swatch_colors` are the four 2×2 preview
/// quadrants (`[TL, TR, BL, BR]`, row-major); `theme_id` maps to the live
/// `active_theme_id` + persistence; `group` drives the family heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePreset {
    /// Stable preset id (`0..PRESET_COUNT`) == [`BUILTIN_THEMES`] array index.
    pub id: u8,
    /// Live theme id ("dark", "light", …) for `apply_active_theme_by_id` +
    /// `active_theme` persistence.
    pub theme_id: &'static str,
    /// Family group (drives the heading the card sits under).
    pub group: ThemeGroup,
    /// i18n string id for the preset's display name.
    pub name_id: StringId,
    /// 2×2 preview quadrant colours, row-major `[TL, TR, BL, BR]`.
    pub swatch_colors: [Color; 4],
}

/// File-local 4-quadrant swatch constructor. Keeps [`BUILTIN_THEMES`] dense
/// (one preset per block, swatch row on one line) so the 17-entry table stays
/// well under the §15 module-size budget without losing the 1:1 colour values.
/// Order matches [`ThemePreset::swatch_colors`] — `[TL, TR, BL, BR]`.
const fn sw(tl: u32, tr: u32, bl: u32, br: u32) -> [Color; 4] {
    [color(tl), color(tr), color(bl), color(br)]
}

/// `0xRRGGBBAA` → [`Color`]. Lets each swatch quadrant read as the literal hex
/// transcribed from Tauri `presets.ts` (alpha in the low byte).
const fn color(rgba: u32) -> Color {
    Color::from_u8(
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    )
}

/// The 17 built-in theme presets, in `THEME_GROUP_ORDER` render order
/// (Rounded 9 → Solid 1 → Angular 4 → Personality 3). `id` is the flat array
/// index so it stays stable; the paint loop walks groups. Colours transcribed
/// 1:1 from Tauri `presets.ts` `preview_colors` (alpha honoured — `frosted`
/// has two translucent quadrants `0x26` ≈ 0.15 / `0x40` ≈ 0.25). Swatch order
/// is `[TL, TR, BL, BR]`; each value is `0xRRGGBBAA`.
pub const BUILTIN_THEMES: [ThemePreset; PRESET_COUNT] = [
    // ── Rounded Glass (9) ────────────────────────────────────────────────
    ThemePreset {
        id: 0,
        theme_id: "dark",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_DARK,
        swatch_colors: sw(0x12121AFF, 0x3B82F6FF, 0xF0F0F5FF, 0x1A1A24FF),
    },
    ThemePreset {
        id: 1,
        theme_id: "light",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_LIGHT,
        swatch_colors: sw(0xFAFAFCFF, 0x3B82F6FF, 0x111118FF, 0xFFFFFFFF),
    },
    ThemePreset {
        id: 2,
        theme_id: "midnight",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_MIDNIGHT,
        swatch_colors: sw(0x0F172AFF, 0x6366F1FF, 0xE2E8F0FF, 0x1E293BFF),
    },
    ThemePreset {
        id: 3,
        theme_id: "forest",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_FOREST,
        swatch_colors: sw(0x1A2E1AFF, 0x22C55EFF, 0xE8F5E9FF, 0x2D4A2DFF),
    },
    ThemePreset {
        id: 4,
        theme_id: "sunset",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_SUNSET,
        swatch_colors: sw(0x2A1A0AFF, 0xF59E0BFF, 0xFEF3C7FF, 0x3D2B16FF),
    },
    // `frosted` TL/BR are translucent (0x26 ≈ rgba .15 / 0x40 ≈ rgba .25).
    ThemePreset {
        id: 5,
        theme_id: "frosted",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_FROSTED,
        swatch_colors: sw(0xFFFFFF26, 0x60A5FAFF, 0xF0F0F5FF, 0xFFFFFF40),
    },
    ThemePreset {
        id: 6,
        theme_id: "ocean-blue",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_OCEAN_BLUE,
        swatch_colors: sw(0x082F49FF, 0x0EA5E9FF, 0xE0F2FEFF, 0x0C4A6EFF),
    },
    ThemePreset {
        id: 7,
        theme_id: "rose-gold",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_ROSE_GOLD,
        swatch_colors: sw(0x4C1D27FF, 0xF43F5EFF, 0xFFF1F2FF, 0x881337FF),
    },
    ThemePreset {
        id: 8,
        theme_id: "forest-green",
        group: ThemeGroup::Rounded,
        name_id: ids::THEME_NAME_FOREST_GREEN,
        swatch_colors: sw(0x142E1AFF, 0x22C55EFF, 0xDCFCE7FF, 0x166534FF),
    },
    // ── Solid (1) ────────────────────────────────────────────────────────
    ThemePreset {
        id: 9,
        theme_id: "solid",
        group: ThemeGroup::Solid,
        name_id: ids::THEME_NAME_SOLID,
        swatch_colors: sw(0x1E1E2EFF, 0x89B4FAFF, 0xCDD6F4FF, 0x313244FF),
    },
    // ── Angular Modern (4) ─────────────────────────────────────────────────
    ThemePreset {
        id: 10,
        theme_id: "order",
        group: ThemeGroup::Angular,
        name_id: ids::THEME_NAME_ORDER,
        swatch_colors: sw(0xFF512FFF, 0xFAFAFAFF, 0x1F2937FF, 0xCBD5E1FF),
    },
    ThemePreset {
        id: 11,
        theme_id: "flat",
        group: ThemeGroup::Angular,
        name_id: ids::THEME_NAME_FLAT,
        swatch_colors: sw(0xE74C3CFF, 0x2C3E50FF, 0xECF0F1FF, 0x3498DBFF),
    },
    ThemePreset {
        id: 12,
        theme_id: "brutalism",
        group: ThemeGroup::Angular,
        name_id: ids::THEME_NAME_BRUTALISM,
        swatch_colors: sw(0xFFD400FF, 0x000000FF, 0xFFFFFFFF, 0xE63946FF),
    },
    ThemePreset {
        id: 13,
        theme_id: "editorial",
        group: ThemeGroup::Angular,
        name_id: ids::THEME_NAME_EDITORIAL,
        swatch_colors: sw(0xFAFAFAFF, 0x0A0A0AFF, 0xD7263DFF, 0xE5E5E5FF),
    },
    // ── Personality (3) ────────────────────────────────────────────────────
    ThemePreset {
        id: 14,
        theme_id: "neo",
        group: ThemeGroup::Personality,
        name_id: ids::THEME_NAME_NEO,
        swatch_colors: sw(0x667EEAFF, 0xE6E8EEFF, 0x2D3748FF, 0xFFFFFFFF),
    },
    ThemePreset {
        id: 15,
        theme_id: "terminal",
        group: ThemeGroup::Personality,
        name_id: ids::THEME_NAME_TERMINAL,
        swatch_colors: sw(0x0A0E0CFF, 0x00FF9CFF, 0x050705FF, 0x003D24FF),
    },
    ThemePreset {
        id: 16,
        theme_id: "cyberpunk",
        group: ThemeGroup::Personality,
        name_id: ids::THEME_NAME_CYBERPUNK,
        swatch_colors: sw(0x0C0420FF, 0x00F0FFFF, 0xFF2E93FF, 0x1A0B3BFF),
    },
];

/// VIBRANT accent-swatch palette (Control B MVP, §7). The 12 hex values are
/// transcribed from Tauri `accentPresets.ts` `VIBRANT.colors`; the inline
/// strip lets the user pick an accent without an OS colour dialog.
pub const ACCENT_SWATCHES: [Color; ACCENT_SWATCH_COUNT] = [
    Color::from_u8(0xEF, 0x44, 0x44, 0xFF), // #ef4444
    Color::from_u8(0xF9, 0x73, 0x16, 0xFF), // #f97316
    Color::from_u8(0xF5, 0x9E, 0x0B, 0xFF), // #f59e0b
    Color::from_u8(0xEA, 0xB3, 0x08, 0xFF), // #eab308
    Color::from_u8(0x84, 0xCC, 0x16, 0xFF), // #84cc16
    Color::from_u8(0x22, 0xC5, 0x5E, 0xFF), // #22c55e
    Color::from_u8(0x14, 0xB8, 0xA6, 0xFF), // #14b8a6
    Color::from_u8(0x06, 0xB6, 0xD4, 0xFF), // #06b6d4
    Color::from_u8(0x3B, 0x82, 0xF6, 0xFF), // #3b82f6
    Color::from_u8(0x8B, 0x5C, 0xF6, 0xFF), // #8b5cf6
    Color::from_u8(0xD9, 0x46, 0xEF, 0xFF), // #d946ef
    Color::from_u8(0xEC, 0x48, 0x99, 0xFF), // #ec4899
];

/// The 7-character lowercase hex string for accent swatch `index`
/// (`#rrggbb`). Returns `None` for an out-of-range index. Used by the shell to
/// persist the picked accent without formatting (allocation-free, `&'static`).
pub const fn accent_swatch_hex(index: usize) -> Option<&'static str> {
    // Parallel to `ACCENT_SWATCHES` — the hit-tester returns the index, the
    // shell maps it to the canonical hex for `draft_accent_color` /
    // `Vault::set_setting("accent_color", …)`.
    const HEXES: [&str; ACCENT_SWATCH_COUNT] = [
        "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#14b8a6", "#06b6d4",
        "#3b82f6", "#8b5cf6", "#d946ef", "#ec4899",
    ];
    if index < HEXES.len() {
        Some(HEXES[index])
    } else {
        None
    }
}

// =============================================================================
// Layout output — fixed-cap, Copy, allocation-free (§10).
// =============================================================================

/// Inline §3 Appearance layout in absolute (body scroll-space) DIPs. Every
/// field is `Copy`; the arrays are fixed-size (no `Vec`). `cards` /
/// `swatch_blocks` are indexed by `ThemePreset::id`; `group_headings` by
/// `THEME_GROUP_ORDER` position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppearanceLayout {
    /// Group-heading rects, indexed by [`THEME_GROUP_ORDER`] position.
    pub group_headings: [Rect; 4],
    /// Per-preset card rects, indexed by [`ThemePreset::id`].
    pub cards: [Rect; PRESET_COUNT],
    /// 40×40 swatch block rects (centred inside each card), by preset id.
    pub swatch_blocks: [Rect; PRESET_COUNT],
    /// Accent row container rect (label + compact colour control).
    pub accent_row: Rect,
    /// Retired inline `#rrggbb` editor rect (zero-sized for compatibility).
    pub accent_input: Rect,
    /// Native colour-dialog launcher rect, matching Tauri's 36×28 input.
    pub accent_picker: Rect,
    /// Retired inline accent reset rect (zero-sized for compatibility).
    pub accent_clear: Rect,
    /// Retired quick-swatch rects (zero-sized for compatibility).
    pub accent_swatches: [Rect; ACCENT_SWATCH_COUNT],
    /// Total laid-out height (grid + accent row) for the body content height.
    pub total_height: f32,
}

// =============================================================================
// Layout function — pure / allocation-free.
// =============================================================================

/// Build an [`AppearanceLayout`] anchored at `origin` (top-left of the section
/// in body scroll-space) with the given content `inner_w` (the body width
/// minus section padding). Walks the four families in [`THEME_GROUP_ORDER`],
/// placing each family's cards in a 4-column grid under its heading, then the
/// accent row below the grid. Allocation-free; safe to call every frame.
pub fn appearance_layout(origin: Point, inner_w: f32) -> AppearanceLayout {
    let cols = THEME_GRID_COLS as f32;
    let card_w = ((inner_w - (cols - 1.0) * THEME_GRID_GAP) / cols).max(SWATCH_BLOCK_SIZE);

    let mut cards = [Rect::ZERO; PRESET_COUNT];
    let mut swatch_blocks = [Rect::ZERO; PRESET_COUNT];
    let mut group_headings = [Rect::ZERO; 4];

    let mut y = origin.y;
    let mut group_pos = 0usize;
    while group_pos < THEME_GROUP_ORDER.len() {
        let group = THEME_GROUP_ORDER[group_pos];
        // Group heading sits at the top of the family block.
        group_headings[group_pos] = Rect {
            x: origin.x,
            y,
            width: inner_w,
            height: GROUP_HEADING_HEIGHT,
        };
        let grid_top = y + GROUP_HEADING_HEIGHT + GROUP_HEADING_TO_GRID_GAP;

        // Walk the preset table; place every card whose group matches, in flat
        // id order (== array order), into this family's grid. `cell` counts
        // matched cards so the row/col is family-local.
        let mut cell = 0usize;
        let mut i = 0usize;
        while i < PRESET_COUNT {
            if BUILTIN_THEMES[i].group as u8 == group as u8 {
                let row = cell / THEME_GRID_COLS;
                let col = cell % THEME_GRID_COLS;
                let card = Rect {
                    x: origin.x + (col as f32) * (card_w + THEME_GRID_GAP),
                    y: grid_top + (row as f32) * (THEME_CARD_HEIGHT + THEME_GRID_GAP),
                    width: card_w,
                    height: THEME_CARD_HEIGHT,
                };
                cards[i] = card;
                // Swatch block — 40×40, centred horizontally, pad-top from card.
                swatch_blocks[i] = Rect {
                    x: card.x + (card.width - SWATCH_BLOCK_SIZE) * 0.5,
                    y: card.y + THEME_CARD_PAD_TOP,
                    width: SWATCH_BLOCK_SIZE,
                    height: SWATCH_BLOCK_SIZE,
                };
                cell += 1;
            }
            i += 1;
        }

        // Advance y past this family's grid. ceil(cell / cols) rows.
        let rows = cell.div_ceil(THEME_GRID_COLS);
        let grid_h = if rows == 0 {
            0.0
        } else {
            (rows as f32) * THEME_CARD_HEIGHT + (rows as f32 - 1.0) * THEME_GRID_GAP
        };
        y = grid_top + grid_h + GROUP_TO_GROUP_GAP;
        group_pos += 1;
    }

    // Accent row sits below the last family's grid (drop the trailing
    // group-to-group gap, add the explicit accent top gap instead).
    let accent_row_y = y - GROUP_TO_GROUP_GAP + ACCENT_ROW_TOP_GAP;
    let accent_row = Rect {
        x: origin.x,
        y: accent_row_y,
        width: inner_w,
        height: ACCENT_ROW_HEIGHT,
    };
    // Match Tauri's single 36×28 colour input. Nano reuses the existing
    // ChooseColorW producer; the old inline hex / clear / quick-swatch state
    // remains compatible but no longer competes with the primary control.
    let accent_picker = Rect {
        x: accent_row.right() - ACCENT_PICKER_W,
        y: accent_row.y + (accent_row.height - ACCENT_PICKER_H) * 0.5,
        width: ACCENT_PICKER_W,
        height: ACCENT_PICKER_H,
    };
    let accent_input = Rect::ZERO;
    let accent_clear = Rect::ZERO;
    let accent_swatches = [Rect::ZERO; ACCENT_SWATCH_COUNT];

    let total_height = accent_row.bottom() - origin.y;

    AppearanceLayout {
        group_headings,
        cards,
        swatch_blocks,
        accent_row,
        accent_input,
        accent_picker,
        accent_clear,
        accent_swatches,
        total_height,
    }
}

/// Total laid-out height of the §3 Appearance section for the given content
/// width — fed into `settings_body_content_height` so the scroll clamp matches
/// what is painted. Equals `appearance_layout(_, inner_w).total_height`; the
/// section anchor Y does not affect the height.
pub fn appearance_content_height(inner_w: f32) -> f32 {
    appearance_layout(Point::ZERO, inner_w).total_height
}

/// Compute the four 2×2 swatch quadrant rects inside `block`. Order: top-left,
/// top-right, bottom-left, bottom-right — matches `ThemePreset::swatch_colors`
/// indexing. The quadrants share a centred `gutter`-DIP divider (Tauri
/// `.theme-card__swatches { gap: 3px }` → pass [`SWATCH_INNER_GAP`]) so the
/// four colours read distinctly even when similar.
pub fn thumbnail_swatch_quadrants(block: Rect, gutter: f32) -> [Rect; 4] {
    let half_w = ((block.width - gutter) * 0.5).max(0.0);
    let half_h = ((block.height - gutter) * 0.5).max(0.0);
    let right_x = block.x + half_w + gutter;
    let bottom_y = block.y + half_h + gutter;
    [
        Rect {
            x: block.x,
            y: block.y,
            width: half_w,
            height: half_h,
        },
        Rect {
            x: right_x,
            y: block.y,
            width: half_w,
            height: half_h,
        },
        Rect {
            x: block.x,
            y: bottom_y,
            width: half_w,
            height: half_h,
        },
        Rect {
            x: right_x,
            y: bottom_y,
            width: half_w,
            height: half_h,
        },
    ]
}

// =============================================================================
// Hit testing
// =============================================================================

/// Hit-test result for a cursor against an [`AppearanceLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceHit {
    /// Cursor fell on the ThemeCard with this preset id (`0..PRESET_COUNT`).
    Card(u8),
    /// Cursor fell on the accent swatch with this strip index
    /// (`0..ACCENT_SWATCH_COUNT`).
    Accent(u8),
    /// Cursor fell on the inline `#rrggbb` accent editor.
    AccentEditor,
    /// Cursor fell on the native OS colour dialog launcher.
    AccentPicker,
    /// Cursor fell on the inline accent reset button.
    AccentClear,
}

/// Hit-test `(x, y)` against `layout`. Cards take priority over accent
/// swatches (spatially disjoint anyway). `None` when the cursor sits in the
/// section but outside every interactive rect. Allocation-free.
pub fn appearance_hit_test(layout: &AppearanceLayout, x: f32, y: f32) -> Option<AppearanceHit> {
    let mut i = 0;
    while i < PRESET_COUNT {
        if rect_contains(layout.cards[i], x, y) {
            return Some(AppearanceHit::Card(i as u8));
        }
        i += 1;
    }
    if rect_contains(layout.accent_input, x, y) {
        return Some(AppearanceHit::AccentEditor);
    }
    if rect_contains(layout.accent_picker, x, y) {
        return Some(AppearanceHit::AccentPicker);
    }
    if rect_contains(layout.accent_clear, x, y) {
        return Some(AppearanceHit::AccentClear);
    }
    let mut s = 0;
    while s < ACCENT_SWATCH_COUNT {
        if rect_contains(layout.accent_swatches[s], x, y) {
            return Some(AppearanceHit::Accent(s as u8));
        }
        s += 1;
    }
    None
}

#[inline]
fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests;
