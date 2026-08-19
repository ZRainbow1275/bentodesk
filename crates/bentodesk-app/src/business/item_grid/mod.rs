//! Business surface — `ItemGrid`, the per-zone tile layout.
//!
//! Visual spec: see `item_grid.snap.md`. Picks between `Direct` (small zone),
//! `Virtual` (≥ 50 items) and `Empty` modes via `pick_layout`. Geometry
//! constants are locked here so re-layout never goes through string keys.
//!
//! The native renderer, hit-test and drag geometry consume these constants and
//! helpers as their shared grid model. `build()` is the compatibility
//! widget-tree descriptor; item painting is owned by `render::item_cards`.

use bentodesk_layout::Direction;
use bentodesk_style::Length;
use bentodesk_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};

/// Item count above which the grid switches to virtualized rendering.
/// Mirrors 1.x `VIRTUAL_THRESHOLD = 50` from `ItemGrid.tsx`.
pub const ITEM_GRID_VIRTUAL_THRESHOLD: usize = 50;

/// Logical-pixel row height inside a virtualized grid. P3.7 (2026-06-02, 1:1):
/// realigned from the legacy 80 to Tauri's 78 (card stride is `78 + 8` gap =
/// 86). 1.x `VirtualItemGrid.tsx` `ROW_HEIGHT` predates the final card height.
pub const ITEM_GRID_ROW_HEIGHT_PX: f32 = 78.0;

/// Number of rows kept rendered above and below the viewport while
/// virtualized — mirrors 1.x `OVERSCAN_ROWS = 3`.
pub const ITEM_GRID_OVERSCAN_ROWS: usize = 3;

/// Inter-column gap (logical px) — locked.
pub const ITEM_GRID_COLUMN_GAP_PX: f32 = 8.0;

/// Inter-row gap (logical px) — locked.
pub const ITEM_GRID_ROW_GAP_PX: f32 = 8.0;

/// Keep the continuous bottom resize strip clear of clipped ItemCards.
pub const ITEM_GRID_BOTTOM_RESIZE_INSET_PX: f32 = 8.0;

/// Vertical offset (logical px) of the first item row below the panel's top
/// edge — i.e. where the item grid starts, immediately under the expanded
/// `PanelHeader` band. P3.6 (2026-06-02, 1:1): Tauri's `.panel-header` is
/// `height: 48px` (PanelHeader.css:6) with `flex-shrink: 0`, and the grid host
/// (`.bento-panel__content`) follows it in the column flex with a
/// `--spacing-sm` (8px) top pad — so the first row begins at `48 + 8 = 56`.
/// The SSoTs are now SPLIT: `expanded_zone_grid::HEADER_BAND_HEIGHT` stays 48
/// (the header band / divider seam) while this grid-top offset is 56 (header +
/// the missing content pad). This is the SSoT shared by the renderer
/// (`highlight_overlay::item_card_rect_for_grid`) and the shell hit-tests, so
/// the painted grid and its hit-rects can never drift (V-13 paint-hit parity).
pub const ITEM_GRID_TOP_OFFSET_PX: f32 = 56.0;

/// Default column count when the zone configuration supplies none.
/// Mirrors 1.x `props.gridColumns ?? 4`.
pub const ITEM_GRID_DEFAULT_COLUMNS: u32 = 4;

/// Smallest expanded width that can contain the configured number of columns
/// without shrinking a card below its 44-DIP interaction target.
pub fn minimum_panel_width(grid_columns: u32) -> f32 {
    let columns = grid_columns.max(1) as f32;
    let inset = crate::expanded_zone_grid::HEADER_INSET_X;
    (inset * 2.0 + (columns - 1.0) * ITEM_GRID_COLUMN_GAP_PX + columns * 44.0).max(80.0)
}

pub const ITEM_ICON_MIN_PX: f32 = 24.0;
pub const ITEM_ICON_MAX_PX: f32 = 48.0;
pub const ITEM_ICON_HOVER_SCALE: f32 = 1.12;
pub const ITEM_LABEL_MIN_FONT_PX: f32 = 11.0;
pub const ITEM_LABEL_MAX_FONT_PX: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResponsiveItemMetrics {
    pub icon_side: f32,
    pub icon_slot_side: f32,
    pub label_font_px: f32,
    pub label_line_height: f32,
    pub label_lines: u32,
    pub required_height: f32,
}

#[inline]
pub fn visible_item_name(name: &str) -> &str {
    let Some(ext) = name.get(name.len().saturating_sub(4)..) else {
        return name;
    };
    if ext.eq_ignore_ascii_case(".lnk") || ext.eq_ignore_ascii_case(".url") {
        name.get(..name.len() - 4).unwrap_or(name)
    } else {
        name
    }
}

/// Responsive ItemCard metrics used by both flow layout and paint. The line
/// count intentionally uses conservative glyph widths so W-heavy, CJK, emoji,
/// and unbroken names reserve enough row height before DWrite wraps them.
pub fn responsive_item_metrics(card_width: f32, name: &str) -> ResponsiveItemMetrics {
    let inner_width = (card_width - 8.0).max(0.0);
    let icon_side = (card_width * 0.36)
        .clamp(ITEM_ICON_MIN_PX, ITEM_ICON_MAX_PX)
        .min(inner_width / ITEM_ICON_HOVER_SCALE);
    let icon_slot_side = icon_side * ITEM_ICON_HOVER_SCALE;
    let label_font_px = (card_width * 0.13).clamp(ITEM_LABEL_MIN_FONT_PX, ITEM_LABEL_MAX_FONT_PX);
    let label_line_height = label_font_px * 1.4;
    let text = visible_item_name(name);
    let max_ems = (inner_width / label_font_px.max(1.0)).max(0.5);
    let mut lines = 1_u32;
    let mut line_ems = 0.0_f32;
    for ch in text.chars() {
        if ch == '\n' {
            lines += 1;
            line_ems = 0.0;
            continue;
        }
        // Deliberately use upper bounds instead of average glyph advances.
        // DWrite may fall back to a different face for CJK/emoji, so every
        // non-ASCII scalar reserves more than a full em. Latin `W`/`M` and
        // lower-case `m` are the reason the former average-width table was not
        // safe for geometry.
        let em = if ch.is_ascii_whitespace() {
            0.5
        } else if ch.is_ascii_uppercase() {
            1.10
        } else if ch.is_ascii_alphanumeric() {
            0.90
        } else if ch.is_ascii() {
            0.75
        } else {
            1.25
        };
        if line_ems > 0.0 && line_ems + em > max_ems {
            lines += 1;
            line_ems = em;
        } else {
            line_ems += em;
        }
    }
    // Wrapped labels reserve one additional safety line. Real DWrite metrics
    // tests below keep this business estimate an upper bound even when font
    // fallback changes glyph advances.
    if lines > 1 {
        lines += 1;
    }
    let required_height = 8.0 + icon_slot_side + 4.0 + label_line_height * lines as f32 + 8.0;
    ResponsiveItemMetrics {
        icon_side,
        icon_slot_side,
        label_font_px,
        label_line_height,
        label_lines: lines,
        required_height: required_height.max(ITEM_GRID_ROW_HEIGHT_PX),
    }
}

/// Layout mode chosen once per zone load — switched only when item count
/// crosses `ITEM_GRID_VIRTUAL_THRESHOLD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    /// Zone is empty; render only the parent BentoPanel chrome.
    Empty,
    /// Direct render of every ItemCard (item_count < threshold).
    Direct,
    /// Virtualized render via VirtualGrid (item_count >= threshold).
    Virtual,
}

/// Pick the layout mode for a zone holding `item_count` items.
/// `_grid_columns` stays in the compatibility API; the current mode depends
/// only on item count.
pub fn pick_layout(item_count: usize, _grid_columns: u32) -> LayoutMode {
    if item_count == 0 {
        LayoutMode::Empty
    } else if item_count >= ITEM_GRID_VIRTUAL_THRESHOLD {
        LayoutMode::Virtual
    } else {
        LayoutMode::Direct
    }
}

/// Column span for a single item, accounting for the wide-card variant.
/// Mirrors 1.x `style={{ "grid-column": is_wide ? "span 2" : undefined }}`.
pub const fn column_span_for(is_wide: bool) -> u32 {
    if is_wide { 2 } else { 1 }
}

pub fn effective_column_count(_zone_width: f32, requested_columns: u32, _inset_x: f32) -> u32 {
    // The editor value is a layout contract, not a hint. This matches the
    // original `repeat(grid_columns, 1fr)` grid and keeps paint/hit/drop aligned.
    requested_columns.max(1)
}

/// Build the compatibility widget-tree descriptor for the grid host.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_layout::LayoutSource;

    #[test]
    fn locked_constants_match_snap_md() {
        assert_eq!(ITEM_GRID_VIRTUAL_THRESHOLD, 50);
        // P3.7 (1:1) — card row height is Tauri's 78 (was the legacy 80).
        assert!((ITEM_GRID_ROW_HEIGHT_PX - 78.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_OVERSCAN_ROWS, 3);
        assert!((ITEM_GRID_COLUMN_GAP_PX - 8.0).abs() < 0.01);
        assert!((ITEM_GRID_ROW_GAP_PX - 8.0).abs() < 0.01);
        assert!((ITEM_GRID_BOTTOM_RESIZE_INSET_PX - 8.0).abs() < 0.01);
        assert_eq!(ITEM_GRID_DEFAULT_COLUMNS, 4);
        // P3.6 (1:1) — grid starts at the 48-DIP Tauri `.panel-header` plus the
        // 8-DIP `--spacing-sm` content pad = 56.
        assert!((ITEM_GRID_TOP_OFFSET_PX - 56.0).abs() < 0.01);
    }

    #[test]
    fn effective_column_count_preserves_the_user_setting() {
        assert_eq!(effective_column_count(320.0, 5, 16.0), 5);
        assert_eq!(effective_column_count(320.0, 6, 16.0), 6);
        assert_eq!(effective_column_count(320.0, 4, 16.0), 4);
        assert_eq!(effective_column_count(240.0, 5, 16.0), 5);
        assert_eq!(effective_column_count(720.0, 5, 16.0), 5);
        assert_eq!(effective_column_count(80.0, 5, 16.0), 5);
        assert_eq!(effective_column_count(320.0, 0, 16.0), 1);
    }

    #[test]
    fn minimum_panel_width_contains_every_configured_card_target() {
        assert_eq!(minimum_panel_width(1), 80.0);
        assert_eq!(minimum_panel_width(4), 232.0);
        assert_eq!(minimum_panel_width(6), 336.0);
    }

    #[test]
    fn responsive_metrics_reserve_wrapped_long_and_non_ascii_labels() {
        let short = responsive_item_metrics(96.0, "A.txt");
        let long = responsive_item_metrics(52.0, "WWWWWWWWWWWWWWWWWWWW.txt");
        let cjk = responsive_item_metrics(52.0, "超长文件名字测试测试测试.txt");
        assert!((ITEM_ICON_MIN_PX..=ITEM_ICON_MAX_PX).contains(&short.icon_side));
        assert!((ITEM_LABEL_MIN_FONT_PX..=ITEM_LABEL_MAX_FONT_PX).contains(&short.label_font_px));
        assert!(long.label_lines > 1 && long.required_height > ITEM_GRID_ROW_HEIGHT_PX);
        assert!(cjk.label_lines > 1 && cjk.required_height > ITEM_GRID_ROW_HEIGHT_PX);
        assert_eq!(
            short.icon_slot_side,
            short.icon_side * ITEM_ICON_HOVER_SCALE
        );
    }

    #[test]
    fn responsive_line_budget_upper_bounds_real_dwrite_wrapping() {
        use bentodesk_platform::dwrite;

        for name in [
            "WWWWWWWWWWWWWWWWWWWWWWWW.txt",
            "Mixed Wide MWmw Latin 2026.txt",
            "mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm.txt",
            "超长文件名字测试测试测试测试.txt",
            "📁🚀🧭✨📦🖥️🗂️.txt",
            "unbrokenfilenamewithmanywidelettersMWMWMWMW.txt",
        ] {
            let card_width = 52.0;
            let metrics = responsive_item_metrics(card_width, name);
            let format = dwrite::text_format_from_family_name_with_metrics(
                "Segoe UI",
                metrics.label_font_px,
                400,
                1.4,
                dwrite::locale_zh_cn(),
            )
            .expect("item label text format");
            let utf16 = visible_item_name(name).encode_utf16().collect::<Vec<_>>();
            let layout = dwrite::create_layout(
                &utf16,
                &format,
                (card_width - 8.0).max(1.0),
                4096.0,
                dwrite::TextAlign::DEFAULT,
            )
            .expect("item label layout");
            let mut actual_lines = 0_u32;
            // SAFETY: `layout` is live; a missing buffer asks DWrite only for
            // the required line count and writes it to `actual_lines`. Windows
            // reports `ERROR_INSUFFICIENT_BUFFER` for that sizing call.
            let _ = unsafe { layout.GetLineMetrics(None, &mut actual_lines) };
            assert!(
                actual_lines > 0,
                "{name:?}: DWrite returned no line metrics"
            );
            let mut line_metrics = vec![
                windows::Win32::Graphics::DirectWrite::DWRITE_LINE_METRICS::default();
                actual_lines as usize
            ];
            // SAFETY: the vector has exactly the entry count requested above.
            unsafe {
                layout
                    .GetLineMetrics(Some(&mut line_metrics), &mut actual_lines)
                    .expect("item label line metrics");
            }
            assert!(
                actual_lines <= metrics.label_lines,
                "{name:?}: DWrite used {actual_lines} lines, budget reserved {}",
                metrics.label_lines
            );
        }
    }

    #[test]
    fn pick_layout_empty_when_zero() {
        assert_eq!(pick_layout(0, 4), LayoutMode::Empty);
    }

    #[test]
    fn pick_layout_direct_below_threshold() {
        assert_eq!(pick_layout(1, 4), LayoutMode::Direct);
        assert_eq!(pick_layout(49, 4), LayoutMode::Direct);
    }

    #[test]
    fn pick_layout_virtual_at_and_above_threshold() {
        assert_eq!(pick_layout(50, 4), LayoutMode::Virtual);
        assert_eq!(pick_layout(500, 4), LayoutMode::Virtual);
    }

    #[test]
    fn column_span_wide_takes_two() {
        assert_eq!(column_span_for(false), 1);
        assert_eq!(column_span_for(true), 2);
    }

    #[test]
    fn build_produces_a_container() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Column);
    }

    /// Wire-format lock: layout snapshots persisted in 1.x JSON should
    /// continue to round-trip after the rewrite.
    #[test]
    fn layout_mode_serde_round_trip() {
        for v in [LayoutMode::Empty, LayoutMode::Direct, LayoutMode::Virtual] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: LayoutMode = serde_json::from_str(&s).unwrap_or(LayoutMode::Empty);
            assert_eq!(v, back);
        }
    }
}
