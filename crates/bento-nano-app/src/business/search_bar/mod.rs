//! Business surface: global SearchBar.
//!
//! Visual spec: `search_bar.snap.md`. The selected-stack runtime owns a
//! native D2D Search HWND: keyboard producers update this state machine, the
//! shell queries `bento_nano_backend::search`, and the renderer reads real
//! result rows from here.

use bento_nano_backend::search::SearchItemKind;
use bento_nano_layout::Direction;
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bento_nano_theme::{PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use bento_nano_widget::{ContainerNode, WidgetNode};
use smallvec::SmallVec;
use smol_str::SmolStr;

/// Default window geometry per snap.md.
pub const WINDOW_WIDTH: f32 = 560.0;
pub const INPUT_HEIGHT: f32 = 48.0;
pub const RESULT_ROW_HEIGHT: f32 = 44.0;

/// Debounce window before the typed query fires `Command::QuerySearch`.
pub const DEBOUNCE_MS: u32 = 120;

/// Hard cap on visible result rows.
pub const MAX_VISIBLE_RESULTS: usize = 8;

/// Maximum query length accepted by the selected-stack keyboard path.
pub const MAX_QUERY_CHARS: usize = 96;

/// Runtime panel margin inside the Search aux HWND.
pub const RUNTIME_PANEL_MARGIN_PX: f32 = 18.0;

/// Runtime panel height in DIPs.
pub const RUNTIME_PANEL_HEIGHT_PX: f32 = 500.0;

/// Runtime header height in DIPs.
pub const RUNTIME_HEADER_HEIGHT_PX: f32 = 54.0;

/// Runtime status row height in DIPs.
pub const RUNTIME_STATUS_HEIGHT_PX: f32 = 24.0;

/// Runtime search row stride in DIPs.
pub const RUNTIME_ROW_STRIDE_PX: f32 = 48.0;

/// Runtime close button size.
pub const RUNTIME_CLOSE_BUTTON_SIZE_PX: f32 = 58.0;

/// SearchBar colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchBarChrome {
    /// Drop shadow descriptor drawn behind the search panel.
    pub panel_shadow: Shadow,
    /// Search panel radius.
    pub panel_radius: BorderRadius,
    /// Query input radius.
    pub input_radius: BorderRadius,
    /// Result row radius.
    pub row_radius: BorderRadius,
    /// Close button radius.
    pub close_radius: BorderRadius,
    /// Panel fill colour.
    pub panel_background: Color,
    /// Query input fill colour.
    pub input_background: Color,
    /// Default result row fill colour.
    pub row_background: Color,
    /// Selected result row fill colour.
    pub selected_background: Color,
    /// Close button fill colour.
    pub danger_background: Color,
    /// Title text colour.
    pub title_color: Color,
    /// Primary body text colour.
    pub body_color: Color,
    /// Secondary/muted text colour.
    pub muted_color: Color,
}

impl SearchBarChrome {
    /// Build SearchBar chrome from the currently active app palette.
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    /// Build SearchBar chrome from explicit active theme token groups.
    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            input_radius: radius.lg,
            row_radius: radius.lg,
            close_radius: radius.lg,
            panel_background: palette.surface,
            input_background: palette.surface_alt,
            row_background: palette.surface_alt,
            selected_background: palette.selection,
            danger_background: palette.danger,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
        }
    }

    /// Build SearchBar chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `search-bar-and-suggestor.md` + Wave B `token-mapping.md`):
    /// - panel bg ← `surface_expanded` (the modal-anchored shell)
    /// - input + row bg ← `surface_subtle` (1.x `--surface-subtle` for inputs)
    /// - selected row bg ← `surface_active`
    /// - danger (close) bg ← `accent_red`
    /// - text primary ← `text_primary`; muted ← `text_muted`
    /// - panel radius ← `expanded` (16); input/row/close ← `card` (10)
    /// - panel shadow ← `expanded` outer layer
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            input_radius: BorderRadius::all(radius.card),
            row_radius: BorderRadius::all(radius.card),
            close_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            input_background: palette.surface_subtle,
            row_background: palette.surface_subtle,
            selected_background: palette.surface_active,
            danger_background: palette.accent_red,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
        }
    }
}

/// One search-result row delivered by `bento-nano-backend::search`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Stable id used by `Command::ActivateSearchResult`.
    pub id: SmolStr,
    /// Backend result kind used for activation routing.
    pub kind: SearchItemKind,
    /// Display name (file basename / zone title / settings key).
    pub name: SmolStr,
    /// Breadcrumb / parent path shown in the dim text below the name.
    pub breadcrumb: SmolStr,
    /// Icon slug (Lucide name or `custom:<uuid>`) passed to FileIcon.
    pub icon: SmolStr,
    /// Backend score (higher = better). Display-only in the D2D panel.
    pub score: u32,
    /// Indexed token that matched the query.
    pub matched_token: SmolStr,
}

/// Pointer hit target in the runtime D2D Search panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarPointerHit {
    Row(usize),
    Close,
}

/// SearchBar runtime state. Holds the typed query, latest backend result
/// envelope, keyboard cursor, and debounce timestamp.
#[derive(Debug, Default)]
pub struct SearchBarState {
    pub query: SmolStr,
    pub results: SmallVec<[SearchHit; 8]>,
    pub selected: Option<usize>,
    /// Wall-clock ms since SearchBar opened, captured at the most recent
    /// keystroke. `None` means the debounce timer is idle.
    pub pending_query_at_ms: Option<u32>,
    /// Monotonic ms accumulator the controller advances each frame.
    pub now_ms: u32,
}

impl SearchBarState {
    /// Replace query, reset the debounce timer, and clear stale selection.
    pub fn set_query(&mut self, query: SmolStr) {
        self.query = query;
        self.pending_query_at_ms = Some(self.now_ms);
        self.selected = None;
    }

    /// Append one printable character to the query, respecting
    /// [`MAX_QUERY_CHARS`]. Returns `true` when the query changed.
    pub fn append_char(&mut self, character: char) -> bool {
        if character.is_control() || self.query.chars().count() >= MAX_QUERY_CHARS {
            return false;
        }
        let mut next_query = self.query.to_string();
        next_query.push(character);
        self.set_query(SmolStr::new(next_query));
        true
    }

    /// Delete the last Unicode scalar from the query. Returns `true` when
    /// the query changed.
    pub fn backspace(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        let mut next_query = self.query.to_string();
        if next_query.pop().is_none() {
            return false;
        }
        self.set_query(SmolStr::new(next_query));
        true
    }

    /// Clear query + results and idle the debounce timer.
    pub fn clear(&mut self) {
        self.query = SmolStr::new_static("");
        self.results.clear();
        self.selected = None;
        self.pending_query_at_ms = None;
    }

    /// Advance the wall clock and return `Some(query)` when the debounce
    /// window has elapsed since the last keystroke.
    pub fn tick(&mut self, dt_ms: u32) -> Option<SmolStr> {
        self.now_ms = self.now_ms.saturating_add(dt_ms);
        if let Some(at_ms) = self.pending_query_at_ms {
            if self.now_ms.saturating_sub(at_ms) >= DEBOUNCE_MS {
                self.pending_query_at_ms = None;
                return Some(self.query.clone());
            }
        }
        None
    }

    /// Replace results from the backend. Truncates to
    /// [`MAX_VISIBLE_RESULTS`] so the inline `SmallVec` stays bounded.
    pub fn set_results(&mut self, mut hits: SmallVec<[SearchHit; 8]>) {
        if hits.len() > MAX_VISIBLE_RESULTS {
            hits.truncate(MAX_VISIBLE_RESULTS);
        }
        self.results = hits;
        self.selected = if self.results.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Move the keyboard cursor down one row, wrapping at the end.
    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            self.selected = None;
            return;
        }
        let next = match self.selected {
            None => 0,
            Some(index) => (index + 1) % self.results.len(),
        };
        self.selected = Some(next);
    }

    /// Move the keyboard cursor up one row, wrapping at the start.
    pub fn select_prev(&mut self) {
        if self.results.is_empty() {
            self.selected = None;
            return;
        }
        let previous = match self.selected {
            None => self.results.len() - 1,
            Some(0) => self.results.len() - 1,
            Some(index) => index - 1,
        };
        self.selected = Some(previous);
    }

    /// Select a concrete visible row. Returns `false` when the index is
    /// outside the result envelope.
    pub fn select_index(&mut self, row_index: usize) -> bool {
        if row_index >= self.results.len() {
            return false;
        }
        self.selected = Some(row_index);
        true
    }

    /// Number of rendered result rows.
    pub fn visible_count(&self) -> usize {
        self.results.len().min(MAX_VISIBLE_RESULTS)
    }

    /// Currently selected row index, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Return the currently selected hit.
    pub fn current_hit(&self) -> Option<&SearchHit> {
        self.selected.and_then(|index| self.results.get(index))
    }
}

/// Build the SearchBar subtree. The runtime D2D renderer owns the rich body,
/// but the widget node remains as the business-surface reachability anchor.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(WINDOW_WIDTH),
        height: Length::Auto,
        padding: Edges::ZERO,
        ..ContainerNode::default()
    })
}

/// Runtime panel rectangle shared by renderer and shell hit-testing.
pub fn search_panel_rect(viewport: Size) -> Rect {
    let width = WINDOW_WIDTH.min((viewport.width - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(360.0));
    let height =
        RUNTIME_PANEL_HEIGHT_PX.min((viewport.height - (RUNTIME_PANEL_MARGIN_PX * 2.0)).max(260.0));
    Rect {
        x: ((viewport.width - width) * 0.5).max(RUNTIME_PANEL_MARGIN_PX),
        y: RUNTIME_PANEL_MARGIN_PX,
        width,
        height,
    }
}

/// Runtime panel shadow rectangle.
pub fn search_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Runtime query input rectangle.
pub fn search_input_rect(viewport: Size) -> Rect {
    let panel = search_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + RUNTIME_HEADER_HEIGHT_PX,
        width: panel.width - 36.0,
        height: INPUT_HEIGHT,
    }
}

/// Runtime close button rectangle.
pub fn search_close_rect(viewport: Size) -> Rect {
    let panel = search_panel_rect(viewport);
    Rect {
        x: panel.right() - RUNTIME_CLOSE_BUTTON_SIZE_PX - 14.0,
        y: panel.y + 14.0,
        width: RUNTIME_CLOSE_BUTTON_SIZE_PX,
        height: 26.0,
    }
}

/// Runtime result row rectangle.
pub fn search_row_rect(viewport: Size, row_index: usize) -> Rect {
    let input = search_input_rect(viewport);
    let panel = search_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: input.bottom()
            + RUNTIME_STATUS_HEIGHT_PX
            + 14.0
            + (row_index as f32 * RUNTIME_ROW_STRIDE_PX),
        width: panel.width - 36.0,
        height: RESULT_ROW_HEIGHT,
    }
}

/// Hit-test the runtime D2D Search panel.
pub fn search_hit_test(
    viewport: Size,
    visible_row_count: usize,
    x: f32,
    y: f32,
) -> Option<SearchBarPointerHit> {
    if rect_contains(search_close_rect(viewport), x, y) {
        return Some(SearchBarPointerHit::Close);
    }
    for row_index in 0..visible_row_count.min(MAX_VISIBLE_RESULTS) {
        if rect_contains(search_row_rect(viewport, row_index), x, y) {
            return Some(SearchBarPointerHit::Row(row_index));
        }
    }
    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    fn hit(name: &str) -> SearchHit {
        SearchHit {
            id: SmolStr::new(format!("hit:{name}")),
            kind: SearchItemKind::File,
            name: SmolStr::new(name),
            breadcrumb: SmolStr::new_static(""),
            icon: SmolStr::new_static("file"),
            score: 10,
            matched_token: SmolStr::new(name),
        }
    }

    #[test]
    fn build_returns_searchbar_sized_container() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - WINDOW_WIDTH).abs() < 0.01));
        assert_eq!(layout.direction, Direction::Column);
    }

    #[test]
    fn search_bar_chrome_accepts_explicit_active_palette() {
        let mut palette = bento_nano_theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);

        let chrome = SearchBarChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.input_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(
            chrome.row_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(
            chrome.selected_background,
            Color::from_u8(0x44, 0xAA, 0xEE, 0x66)
        );
        assert_eq!(
            chrome.danger_background,
            Color::from_u8(0xCC, 0x44, 0x44, 0xFF)
        );
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    }

    #[test]
    fn search_bar_chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = bento_nano_theme::current().palette;
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };
        let mut shadow = shadow::DEFAULT;
        shadow.md = Shadow {
            offset_x: 2.0,
            offset_y: 5.0,
            blur: 13.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
        };

        let chrome = SearchBarChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.input_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.close_radius, BorderRadius::all(11.0));
    }

    #[test]
    fn search_panel_shadow_rect_uses_token_shadow_geometry() {
        let panel = Rect {
            x: 24.0,
            y: 30.0,
            width: 320.0,
            height: 180.0,
        };
        let shadow = Shadow {
            offset_x: 3.0,
            offset_y: 5.0,
            blur: 11.0,
            spread: 0.0,
            color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
        };

        let rect = search_panel_shadow_rect(panel, shadow);

        assert_eq!(
            rect,
            Rect {
                x: 16.0,
                y: 24.0,
                width: 342.0,
                height: 202.0,
            }
        );
    }

    #[test]
    fn debounce_holds_until_window_elapses() {
        let mut state = SearchBarState::default();
        state.set_query(SmolStr::new("foo"));
        let fired = state.tick(DEBOUNCE_MS - 1);
        assert!(fired.is_none(), "fired prematurely: {fired:?}");
        let fired = state.tick(2);
        assert_eq!(fired.as_deref(), Some("foo"));
        assert!(state.tick(1000).is_none());
    }

    #[test]
    fn typing_resets_debounce_timer() {
        let mut state = SearchBarState::default();
        state.set_query(SmolStr::new("a"));
        let _ignored = state.tick(DEBOUNCE_MS / 2);
        state.set_query(SmolStr::new("ab"));
        assert!(state.tick(DEBOUNCE_MS / 2).is_none());
        let fired = state.tick(DEBOUNCE_MS);
        assert_eq!(fired.as_deref(), Some("ab"));
    }

    #[test]
    fn results_truncate_to_visible_cap() {
        let mut state = SearchBarState::default();
        let mut many: SmallVec<[SearchHit; 8]> = SmallVec::new();
        for index in 0..(MAX_VISIBLE_RESULTS + 5) {
            many.push(hit(&format!("h{index}")));
        }
        state.set_results(many);
        assert_eq!(state.results.len(), MAX_VISIBLE_RESULTS);
        assert_eq!(state.selected, Some(0));
    }

    #[test]
    fn select_next_wraps_at_end() {
        let mut state = SearchBarState::default();
        let mut hits: SmallVec<[SearchHit; 8]> = SmallVec::new();
        hits.push(hit("a"));
        hits.push(hit("b"));
        state.set_results(hits);
        assert_eq!(state.selected, Some(0));
        state.select_next();
        assert_eq!(state.selected, Some(1));
        state.select_next();
        assert_eq!(state.selected, Some(0));
    }

    #[test]
    fn select_prev_wraps_at_start() {
        let mut state = SearchBarState::default();
        let mut hits: SmallVec<[SearchHit; 8]> = SmallVec::new();
        hits.push(hit("a"));
        hits.push(hit("b"));
        state.set_results(hits);
        state.select_prev();
        assert_eq!(state.selected, Some(1));
    }

    #[test]
    fn empty_results_clear_selection() {
        let mut state = SearchBarState::default();
        let mut hits: SmallVec<[SearchHit; 8]> = SmallVec::new();
        hits.push(hit("a"));
        state.set_results(hits);
        assert_eq!(state.selected, Some(0));
        state.set_results(SmallVec::new());
        assert_eq!(state.selected, None);
        assert!(state.current_hit().is_none());
    }

    #[test]
    fn append_and_backspace_update_query() {
        let mut state = SearchBarState::default();
        assert!(state.append_char('中'));
        assert!(state.append_char('a'));
        assert_eq!(state.query.as_str(), "中a");
        assert!(state.backspace());
        assert_eq!(state.query.as_str(), "中");
        assert!(state.backspace());
        assert!(!state.backspace());
        assert!(state.query.is_empty());
    }

    #[test]
    fn append_rejects_control_chars_and_length_overflow() {
        let mut state = SearchBarState::default();
        assert!(!state.append_char('\n'));
        for _index in 0..MAX_QUERY_CHARS {
            assert!(state.append_char('x'));
        }
        assert!(!state.append_char('y'));
        assert_eq!(state.query.chars().count(), MAX_QUERY_CHARS);
    }

    #[test]
    fn select_index_rejects_out_of_range() {
        let mut state = SearchBarState::default();
        let mut hits: SmallVec<[SearchHit; 8]> = SmallVec::new();
        hits.push(hit("a"));
        state.set_results(hits);
        assert!(state.select_index(0));
        assert!(!state.select_index(1));
        assert_eq!(state.selected_index(), Some(0));
        assert_eq!(state.visible_count(), 1);
    }

    #[test]
    fn runtime_hit_test_distinguishes_row_and_close() {
        let viewport = Size {
            width: 640.0,
            height: 520.0,
        };
        let row = search_row_rect(viewport, 1);
        assert_eq!(
            search_hit_test(viewport, 3, row.x + 2.0, row.y + 2.0),
            Some(SearchBarPointerHit::Row(1))
        );
        let close = search_close_rect(viewport);
        assert_eq!(
            search_hit_test(viewport, 3, close.x + 2.0, close.y + 2.0),
            Some(SearchBarPointerHit::Close)
        );
        assert_eq!(search_hit_test(viewport, 0, row.x + 2.0, row.y + 2.0), None);
    }

    #[test]
    fn debounce_constant_pinned_to_one_x_baseline() {
        assert_eq!(DEBOUNCE_MS, 120);
    }

    #[test]
    fn search_bar_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let chrome = SearchBarChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(chrome.panel_background, style_tokens::PALETTE_DARK.surface_expanded);
        assert_eq!(chrome.input_background, style_tokens::PALETTE_DARK.surface_subtle);
        assert_eq!(chrome.row_background, style_tokens::PALETTE_DARK.surface_subtle);
        assert_eq!(chrome.selected_background, style_tokens::PALETTE_DARK.surface_active);
        assert_eq!(chrome.danger_background, style_tokens::PALETTE_DARK.accent_red);
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.panel_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        assert_eq!(chrome.input_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.row_radius, BorderRadius::all(style_tokens::RADIUS.card));
        assert_eq!(chrome.close_radius, BorderRadius::all(style_tokens::RADIUS.card));
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }
}
