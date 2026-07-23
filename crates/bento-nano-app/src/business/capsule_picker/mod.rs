//! `CapsulePicker` — Context Capsule browser / capture / restore modal.
//!
//! 1.x source: `bentodesk/src/components/CapsulePicker/CapsulePicker.tsx` +
//! `CapsuleCard.tsx`. Modal that lists every saved Context Capsule (each
//! entry = a snapshot of currently open windows + their layout), lets the
//! user capture a new one with a name, and restore / delete existing ones.
//!
//! Visual fidelity reference: `capsule_picker.snap.md`.
//!
//! # Hosting
//!
//! Rendered inside a dedicated `WindowKind::CapsulePicker` HWND
//! (480 × 600 default per `default_size`). The HWND is layered (per
//! `ex_style_for`'s NoRedirectionBitmap path) and accepts focus so the
//! capture-name input field works.
//!
//! # State
//!
//! [`CapsulePickerState`] mirrors the Solid-JS signals from 1.x:
//!   * `entries: SmallVec<[CapsuleEntry; 8]>` — typical user has 1-8
//!     saved capsules; the inline buffer keeps the steady-state alloc-free
//!     (§10).
//!   * `new_name: SmolStr` — the in-progress capture name input.
//!   * `busy: bool` — capture / restore in flight, disables actions.
//!   * `last_error: Option<SmolStr>` — most recent backend error, surfaced
//!     in the error banner.
//!
//! The picker is a *shell* — it doesn't talk to the capsule backend
//! directly. The shell pumps `Command::CaptureCapsule` / `RestoreCapsule` /
//! `DeleteCapsule` through the dispatcher, observes filesystem-backed
//! results, and updates the picker's state.

use core::fmt;

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, ShadowTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Rect, Shadow, Size};
use bento_nano_theme::{self as theme, PaletteTokens, RadiusTokens, ShadowTokens, radius, shadow};
use smallvec::SmallVec;
use smol_str::SmolStr;

/// One row in the picker's capsule list. Mirrors the 1.x `ContextCapsule`
/// interface (id + name + icon + captured-at). The `windows` list of the
/// 1.x type is intentionally NOT mirrored here — the picker only displays
/// the metadata; the windows array is fetched on demand by the restore
/// command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleEntry {
    /// Stable capsule id (UUID v4 string in 1.x; `SmolStr` here so 36-byte
    /// UUIDs heap-allocate cleanly while shorter test ids stay inline).
    pub id: SmolStr,
    /// Display name as captured by the user.
    pub name: SmolStr,
    /// Icon glyph id (Lucide name in 1.x; resolved by the icon family
    /// once T-079 lands).
    pub icon: SmolStr,
    /// ISO-8601 captured-at timestamp.
    pub captured_at: SmolStr,
}

impl CapsuleEntry {
    /// New entry from owned strings. Convenience for shell consumers
    /// translating from a backend response.
    pub fn new(
        id: impl Into<SmolStr>,
        name: impl Into<SmolStr>,
        icon: impl Into<SmolStr>,
        captured_at: impl Into<SmolStr>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            captured_at: captured_at.into(),
        }
    }
}

/// Hover/click target inside the selected-stack CapsulePicker aux window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsulePickerHit {
    /// Save the current Desktop layout as a new capsule.
    Capture,
    /// Restore the selected capsule.
    Restore,
    /// Delete the selected capsule.
    Delete,
    /// Close the picker.
    Close,
    /// Keyboard shortcut hint text.
    Hint,
    /// Visible backend error banner.
    Error,
    /// Empty-state body text.
    Empty,
    /// Visible capsule row by row index.
    Row(usize),
}

/// Maximum capsule rows rendered by the current D2D surface.
pub const CAPSULE_VISIBLE_ROW_LIMIT: usize = 7;

/// Mouse-accessible actions rendered below the helper text.
pub const CAPSULE_PICKER_ACTIONS: [CapsulePickerHit; 4] = [
    CapsulePickerHit::Capture,
    CapsulePickerHit::Restore,
    CapsulePickerHit::Delete,
    CapsulePickerHit::Close,
];

const ACTION_TOP_PX: f32 = 82.0;
const ACTION_HEIGHT_PX: f32 = 30.0;
const ACTION_GAP_PX: f32 = 8.0;
const ROW_TOP_PX: f32 = 128.0;

/// Panel rectangle shared by renderer and hit-test producers.
pub fn capsule_picker_panel_rect(viewport: Size) -> Rect {
    Rect {
        x: 16.0,
        y: 16.0,
        width: (viewport.width - 32.0).max(320.0),
        height: (viewport.height - 32.0).max(280.0),
    }
}

/// Shadow rectangle shared by renderer and any future visual tests.
pub fn capsule_picker_panel_shadow_rect(panel: Rect, shadow: Shadow) -> Rect {
    let spread = shadow.blur.max(0.0);
    Rect {
        x: panel.x + shadow.offset_x - spread,
        y: panel.y + shadow.offset_y - spread,
        width: panel.width + spread * 2.0,
        height: panel.height + spread * 2.0,
    }
}

/// Shortcut hint rectangle shared by renderer and tooltip producers.
pub fn capsule_picker_hint_rect(viewport: Size) -> Rect {
    let panel = capsule_picker_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 52.0,
        width: panel.width - 36.0,
        height: 24.0,
    }
}

/// Error banner rectangle shared by renderer and tooltip producers.
pub fn capsule_picker_error_rect(viewport: Size) -> Rect {
    let panel = capsule_picker_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + 116.0,
        width: panel.width - 36.0,
        height: 22.0,
    }
}

/// Empty-state rectangle shared by renderer and tooltip producers.
pub fn capsule_picker_empty_rect(viewport: Size) -> Rect {
    let panel = capsule_picker_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + panel.height * 0.46,
        width: panel.width - 36.0,
        height: 104.0,
    }
}

/// Action button rectangle shared by rendering and pointer hit-testing.
pub fn capsule_picker_action_rect(viewport: Size, index: usize) -> Rect {
    let panel = capsule_picker_panel_rect(viewport);
    let total_gap = ACTION_GAP_PX * (CAPSULE_PICKER_ACTIONS.len() as f32 - 1.0);
    let width = (panel.width - 36.0 - total_gap) / CAPSULE_PICKER_ACTIONS.len() as f32;
    Rect {
        x: panel.x + 18.0 + index as f32 * (width + ACTION_GAP_PX),
        y: panel.y + ACTION_TOP_PX,
        width,
        height: ACTION_HEIGHT_PX,
    }
}

/// Row rectangle shared by renderer and tooltip producers.
pub fn capsule_picker_row_rect(viewport: Size, index: usize) -> Rect {
    let panel = capsule_picker_panel_rect(viewport);
    Rect {
        x: panel.x + 18.0,
        y: panel.y + ROW_TOP_PX + (index as f32 * 48.0),
        width: panel.width - 36.0,
        height: 40.0,
    }
}

/// Hit-test the currently rendered CapsulePicker geometry.
pub fn capsule_picker_hit_test(
    viewport: Size,
    visible_count: usize,
    has_error: bool,
    x: f32,
    y: f32,
) -> Option<CapsulePickerHit> {
    for (index, hit) in CAPSULE_PICKER_ACTIONS.iter().copied().enumerate() {
        if contains_point(capsule_picker_action_rect(viewport, index), x, y) {
            return Some(hit);
        }
    }
    if contains_point(capsule_picker_hint_rect(viewport), x, y) {
        return Some(CapsulePickerHit::Hint);
    }
    if has_error && contains_point(capsule_picker_error_rect(viewport), x, y) {
        return Some(CapsulePickerHit::Error);
    }
    if visible_count == 0 && contains_point(capsule_picker_empty_rect(viewport), x, y) {
        return Some(CapsulePickerHit::Empty);
    }
    for index in 0..visible_count.min(CAPSULE_VISIBLE_ROW_LIMIT) {
        if contains_point(capsule_picker_row_rect(viewport, index), x, y) {
            return Some(CapsulePickerHit::Row(index));
        }
    }
    None
}

fn contains_point(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.right() && y < rect.bottom()
}

// -----------------------------------------------------------------------------
// Chrome tokens — shared by D2D renderer and descriptor scaffold.
// -----------------------------------------------------------------------------

/// CapsulePicker colour contract derived from an active palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapsulePickerChrome {
    pub panel_shadow: Shadow,
    pub panel_radius: BorderRadius,
    pub row_radius: BorderRadius,
    pub panel_background: Color,
    pub row_background: Color,
    pub selected_background: Color,
    pub title_color: Color,
    pub body_color: Color,
    pub muted_color: Color,
    pub error_color: Color,
}

impl CapsulePickerChrome {
    pub fn from_palette(palette: PaletteTokens) -> Self {
        Self::from_tokens(palette, radius::DEFAULT, shadow::DEFAULT)
    }

    pub fn from_tokens(palette: PaletteTokens, radius: RadiusTokens, shadow: ShadowTokens) -> Self {
        Self {
            panel_shadow: shadow.md,
            panel_radius: radius.xl,
            row_radius: radius.lg,
            panel_background: palette.surface,
            row_background: palette.surface_alt,
            selected_background: palette.selection,
            title_color: palette.text,
            body_color: palette.text,
            muted_color: palette.text_muted,
            error_color: palette.danger,
        }
    }

    /// Build CapsulePicker chrome from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `capsule-picker.md` + Wave B `token-mapping.md`):
    /// - panel bg ← `surface_expanded` (0.82α dark glass per Wave A)
    /// - row bg (idle) ← `surface_hover` (Wave A: list rows lighten on hover)
    /// - selected row bg ← `surface_active`
    /// - title + body ← `text_primary`; muted ← `text_muted`
    /// - error ← `accent_red` (Wave A: error banner colour)
    /// - radii: panel = `expanded` (16), row = `card` (10)
    /// - shadow ← `expanded` (outer)
    pub fn from_tauri_tokens(
        palette: PaletteTauri,
        radius: RadiusTauri,
        shadow: ShadowTauri,
    ) -> Self {
        Self {
            // M6b — `expanded` is a `ShadowStack`; consume the outer layer.
            panel_shadow: shadow.expanded.outer(),
            panel_radius: BorderRadius::all(radius.expanded),
            row_radius: BorderRadius::all(radius.card),
            panel_background: palette.surface_expanded,
            row_background: palette.surface_hover,
            selected_background: palette.surface_active,
            title_color: palette.text_primary,
            body_color: palette.text_primary,
            muted_color: palette.text_muted,
            error_color: palette.accent_red,
        }
    }
}

// -----------------------------------------------------------------------------
// CapsulePicker widget descriptor — chrome of the modal panel.
// -----------------------------------------------------------------------------

/// Modal-panel chrome for the CapsulePicker. The HWND host is the 480 × 600
/// `WindowKind::CapsulePicker` window; this descriptor describes what
/// paints inside it (panel background + title bar + body padding).
#[derive(Debug, Clone)]
pub struct CapsulePicker {
    /// Modal title — `t("capsulePickerTitle")` in 1.x.
    pub title: SmolStr,
    /// Panel background — `palette.surface`.
    pub background: Color,
    /// Title text colour — `palette.text`.
    pub title_color: Color,
    /// Border radius — 12 px matches the 1.x `.capsule-picker` CSS.
    pub border_radius: BorderRadius,
    /// Inset padding around the entire panel content.
    pub padding: Edges,
    /// Panel width — 480 px at 96 DPI baseline.
    pub width: Length,
    /// Panel height — 600 px at 96 DPI baseline.
    pub height: Length,
}

impl CapsulePicker {
    pub fn new(title: impl Into<SmolStr>) -> Self {
        let chrome = CapsulePickerChrome::from_palette(theme::current().palette);
        Self {
            title: title.into(),
            background: chrome.panel_background,
            title_color: chrome.title_color,
            border_radius: chrome.panel_radius,
            padding: Edges::all(20.0),
            width: Length::Px(480.0),
            height: Length::Px(600.0),
        }
    }
}

impl LayoutSource for CapsulePicker {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column: title row → capture row → list scroll → action row.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

// -----------------------------------------------------------------------------
// CapsulePickerState — interactive state mirroring the 1.x signals.
// -----------------------------------------------------------------------------

/// Action emitted by the picker when the user clicks an action button.
/// Drained by the shell once per frame via [`take_action`].
///
/// [`take_action`]: CapsulePickerState::take_action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapsulePickerAction {
    /// User clicked "Capture current". Carries the trimmed capture name
    /// (or a synthesised default if the input was blank).
    Capture(SmolStr),
    /// User clicked "Restore" on a list entry. Carries the entry's id.
    Restore(SmolStr),
    /// User clicked "Delete" on a list entry. Carries the entry's id.
    Delete(SmolStr),
    /// User clicked the close button or the scrim.
    Close,
}

impl fmt::Display for CapsulePickerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(n) => write!(f, "Capture({n:?})"),
            Self::Restore(i) => write!(f, "Restore({i:?})"),
            Self::Delete(i) => write!(f, "Delete({i:?})"),
            Self::Close => f.write_str("Close"),
        }
    }
}

/// Interactive state for the CapsulePicker modal. Owned by the shell or
/// app's `business::capsule_picker` slot; mutated as the user types and
/// clicks; drained per-frame for the latest action.
#[derive(Debug, Default)]
pub struct CapsulePickerState {
    entries: SmallVec<[CapsuleEntry; 8]>,
    selected_index: usize,
    new_name: SmolStr,
    busy: bool,
    last_error: Option<SmolStr>,
    pending_action: Option<CapsulePickerAction>,
}

impl CapsulePickerState {
    /// New empty state. The shell calls [`set_entries`] with the response
    /// from the `list_contexts` backend immediately after.
    ///
    /// [`set_entries`]: CapsulePickerState::set_entries
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the list of capsules — typically called after a `refresh`.
    pub fn set_entries(&mut self, entries: SmallVec<[CapsuleEntry; 8]>) {
        self.entries = entries;
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.entries.len() {
            self.selected_index = self.entries.len() - 1;
        }
    }

    /// Borrow the current entries list.
    pub fn entries(&self) -> &[CapsuleEntry] {
        &self.entries
    }

    /// Currently selected row index.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Select a visible row by mouse index.
    pub fn select_index(&mut self, index: usize) -> bool {
        if index >= self.entries.len() || index == self.selected_index {
            return false;
        }
        self.selected_index = index;
        true
    }

    /// Borrow the selected entry, if any.
    pub fn selected_entry(&self) -> Option<&CapsuleEntry> {
        self.entries.get(self.selected_index)
    }

    /// Select the next visible capsule row.
    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.entries.len();
        }
    }

    /// Select the previous visible capsule row.
    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            self.entries.len() - 1
        } else {
            self.selected_index - 1
        };
    }

    /// User typed in the capture-name input.
    pub fn set_new_name(&mut self, name: impl Into<SmolStr>) {
        self.new_name = name.into();
    }

    /// Borrow the current capture-name input.
    pub fn new_name(&self) -> &str {
        &self.new_name
    }

    /// Mark the picker as busy (during a capture / restore round trip).
    /// While busy, the action buttons render disabled.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// Whether an action is in flight.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Set the most recent backend error message — surfaced in the error
    /// banner. Pass `None` to clear it.
    pub fn set_error(&mut self, msg: Option<SmolStr>) {
        self.last_error = msg;
    }

    /// Borrow the current error message.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// User clicked "Capture current". Builds the capture command from
    /// the current `new_name` (trimmed; falls back to a synthesised
    /// `"Capsule {epoch_ms}"` when blank — the 1.x synthesiser uses
    /// `new Date().toLocaleString()` which we can't easily reproduce in
    /// the widget layer, so we hand the shell an `epoch_ms` placeholder
    /// to stamp the date into).
    ///
    /// `epoch_ms` is the wall-clock time the shell hands in (so this
    /// function stays pure / testable). 1.x synthesises with
    /// `new Date().toLocaleString()`; the shell does the equivalent
    /// `chrono::Utc::now()` formatting before passing in the fallback name.
    pub fn click_capture(&mut self, fallback_name: impl Into<SmolStr>) {
        let trimmed = self.new_name.trim();
        let name: SmolStr = if trimmed.is_empty() {
            fallback_name.into()
        } else {
            SmolStr::new(trimmed)
        };
        self.pending_action = Some(CapsulePickerAction::Capture(name));
    }

    /// User clicked Restore on a capsule row.
    pub fn click_restore(&mut self, id: impl Into<SmolStr>) {
        self.pending_action = Some(CapsulePickerAction::Restore(id.into()));
    }

    /// User clicked Delete on a capsule row.
    pub fn click_delete(&mut self, id: impl Into<SmolStr>) {
        self.pending_action = Some(CapsulePickerAction::Delete(id.into()));
    }

    /// User clicked the close button.
    pub fn click_close(&mut self) {
        self.pending_action = Some(CapsulePickerAction::Close);
    }

    /// Drain the latest action — one-shot. Returns `None` when the user
    /// hasn't clicked anything since the last drain.
    pub fn take_action(&mut self) -> Option<CapsulePickerAction> {
        self.pending_action.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::tokens as style_tokens;

    #[test]
    fn capsule_picker_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
        let chrome = CapsulePickerChrome::from_tauri_tokens(
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SHADOW,
        );
        assert_eq!(
            chrome.panel_background,
            style_tokens::PALETTE_DARK.surface_expanded
        );
        assert_eq!(
            chrome.row_background,
            style_tokens::PALETTE_DARK.surface_hover
        );
        assert_eq!(
            chrome.selected_background,
            style_tokens::PALETTE_DARK.surface_active
        );
        assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.body_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
        assert_eq!(chrome.error_color, style_tokens::PALETTE_DARK.accent_red);
        assert_eq!(
            chrome.panel_radius,
            BorderRadius::all(style_tokens::RADIUS.expanded)
        );
        assert_eq!(
            chrome.row_radius,
            BorderRadius::all(style_tokens::RADIUS.card)
        );
        // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
        assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    }

    fn sample_entry(id: &'static str, name: &'static str) -> CapsuleEntry {
        CapsuleEntry::new(id, name, "briefcase", "2026-05-03T12:00:00Z")
    }

    #[test]
    fn capsule_picker_default_chrome_uses_palette_surface() {
        let p = CapsulePicker::new("Context Capsules");
        let palette = theme::current().palette;
        assert_eq!(p.background, palette.surface);
        assert_eq!(p.title_color, palette.text);
        assert_eq!(p.border_radius.top_left, 12.0);
        assert_eq!(p.width, Length::Px(480.0));
        assert_eq!(p.height, Length::Px(600.0));
    }

    #[test]
    fn capsule_picker_chrome_accepts_explicit_active_palette() {
        let mut palette = theme::current().palette;
        palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
        palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
        palette.selection = Color::from_u8(0x44, 0x55, 0x66, 0xCC);
        palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
        palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
        palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

        let chrome = CapsulePickerChrome::from_palette(palette);

        assert_eq!(
            chrome.panel_background,
            Color::from_u8(0x22, 0x33, 0x44, 0xDD)
        );
        assert_eq!(
            chrome.row_background,
            Color::from_u8(0x11, 0x22, 0x33, 0xEE)
        );
        assert_eq!(
            chrome.selected_background,
            Color::from_u8(0x44, 0x55, 0x66, 0xCC)
        );
        assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
        assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
        assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
    }

    #[test]
    fn capsule_picker_chrome_accepts_explicit_radius_shadow_tokens() {
        let palette = theme::current().palette;
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

        let chrome = CapsulePickerChrome::from_tokens(palette, radius, shadow);

        assert_eq!(chrome.panel_shadow, shadow.md);
        assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
    }

    #[test]
    fn capsule_picker_panel_shadow_rect_uses_token_shadow_geometry() {
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

        let rect = capsule_picker_panel_shadow_rect(panel, shadow);

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
    fn capsule_picker_state_set_entries_replaces_list() {
        let mut s = CapsulePickerState::new();
        let mut entries = SmallVec::new();
        entries.push(sample_entry("a", "Coding"));
        entries.push(sample_entry("b", "Reading"));
        s.set_entries(entries);
        assert_eq!(s.entries().len(), 2);
        assert_eq!(s.entries()[0].name, SmolStr::new_static("Coding"));
        assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("a"));
    }

    #[test]
    fn capsule_picker_selection_wraps_and_clamps() {
        let mut s = CapsulePickerState::new();
        let mut entries = SmallVec::new();
        entries.push(sample_entry("a", "Coding"));
        entries.push(sample_entry("b", "Reading"));
        s.set_entries(entries);
        assert_eq!(s.selected_index(), 0);
        s.select_next();
        assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("b"));
        s.select_next();
        assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("a"));
        s.select_prev();
        assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("b"));

        let mut one = SmallVec::new();
        one.push(sample_entry("z", "Only"));
        s.set_entries(one);
        assert_eq!(s.selected_index(), 0);
        assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("z"));
    }

    #[test]
    fn capsule_picker_mouse_actions_and_row_selection_are_reachable() {
        let viewport = Size {
            width: 480.0,
            height: 600.0,
        };
        for (index, expected) in CAPSULE_PICKER_ACTIONS.iter().copied().enumerate() {
            let rect = capsule_picker_action_rect(viewport, index);
            assert_eq!(
                capsule_picker_hit_test(
                    viewport,
                    2,
                    false,
                    rect.x + rect.width * 0.5,
                    rect.y + rect.height * 0.5,
                ),
                Some(expected)
            );
        }

        let mut state = CapsulePickerState::new();
        let mut entries = SmallVec::new();
        entries.push(sample_entry("a", "Coding"));
        entries.push(sample_entry("b", "Reading"));
        state.set_entries(entries);
        assert!(state.select_index(1));
        assert_eq!(
            state.selected_entry().map(|entry| entry.id.as_str()),
            Some("b")
        );
        assert!(!state.select_index(9));
    }

    #[test]
    fn capsule_picker_click_capture_uses_typed_name() {
        let mut s = CapsulePickerState::new();
        s.set_new_name("My Workflow");
        s.click_capture("ignored fallback");
        assert_eq!(
            s.take_action(),
            Some(CapsulePickerAction::Capture(SmolStr::new_static(
                "My Workflow"
            )))
        );
    }

    #[test]
    fn capsule_picker_click_capture_uses_fallback_when_blank() {
        let mut s = CapsulePickerState::new();
        s.set_new_name("   "); // whitespace only — counts as blank
        s.click_capture("Capsule 2026-05-03 12:00");
        assert_eq!(
            s.take_action(),
            Some(CapsulePickerAction::Capture(SmolStr::new_static(
                "Capsule 2026-05-03 12:00"
            )))
        );
    }

    #[test]
    fn capsule_picker_restore_delete_close_record_actions() {
        let mut s = CapsulePickerState::new();
        s.click_restore("cap-1");
        assert_eq!(
            s.take_action(),
            Some(CapsulePickerAction::Restore(SmolStr::new_static("cap-1")))
        );

        s.click_delete("cap-2");
        assert_eq!(
            s.take_action(),
            Some(CapsulePickerAction::Delete(SmolStr::new_static("cap-2")))
        );

        s.click_close();
        assert_eq!(s.take_action(), Some(CapsulePickerAction::Close));
    }

    #[test]
    fn capsule_picker_take_action_is_one_shot() {
        let mut s = CapsulePickerState::new();
        s.click_close();
        assert!(s.take_action().is_some());
        assert!(s.take_action().is_none());
    }

    #[test]
    fn capsule_picker_busy_and_error_surface_correctly() {
        let mut s = CapsulePickerState::new();
        assert!(!s.is_busy());
        assert!(s.last_error().is_none());

        s.set_busy(true);
        s.set_error(Some(SmolStr::new_static("backend offline")));
        assert!(s.is_busy());
        assert_eq!(s.last_error(), Some("backend offline"));

        s.set_error(None);
        assert!(s.last_error().is_none());
    }
}
