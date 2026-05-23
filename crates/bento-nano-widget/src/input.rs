//! `Input` — single-line text field. IME-aware via the platform layer's
//! `WM_IME_*` handlers; this widget owns the buffer + caret + selection
//! state and exposes the data the renderer needs to draw.
//!
//! Spec §10: `text` is `SmolStr` so short labels (search queries, hotkey
//! names) stay inline; longer entries spill to heap. Caret math is `Copy`.
//! Spec §11: every mutation returns a `Result` or has a clear no-op semantic;
//! no panic on out-of-range positions.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_tree::Signal;
use smol_str::SmolStr;

pub const DEFAULT_HEIGHT_PX: f32 = 32.0;
pub const DEFAULT_WIDTH_PX: f32 = 240.0;

#[derive(Debug)]
pub struct Input {
    /// Reactive buffer. Subscribers (e.g. a SearchBar) re-run their query
    /// when this signal flips dirty.
    pub text: Signal<SmolStr>,
    /// Caret position as a UTF-16 codeunit index — DirectWrite measures in
    /// codeunits. Clamped to `[0, text_len]` on every mutation.
    pub caret: u32,
    /// Selection anchor; equal to `caret` when no selection.
    pub selection_anchor: u32,
    pub width: f32,
    pub height: f32,
    pub padding: Edges,
    pub disabled: bool,
    pub focused: bool,
    pub placeholder: SmolStr,
    /// IME composition string visible to the user before commit. Renderer
    /// underlines this region.
    pub ime_composition: SmolStr,
    pub on_change_event: u32,
    pub on_commit_event: u32,
    pub background: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub caret_color: Color,
    pub selection_bg: Color,
    pub radius: BorderRadius,
}

impl Input {
    pub fn new(placeholder: impl Into<SmolStr>) -> Self {
        let p = theme::current().palette;
        Self {
            text: Signal::new(SmolStr::default()),
            caret: 0,
            selection_anchor: 0,
            width: DEFAULT_WIDTH_PX,
            height: DEFAULT_HEIGHT_PX,
            padding: Edges::xy(8.0, 6.0),
            disabled: false,
            focused: false,
            placeholder: placeholder.into(),
            ime_composition: SmolStr::default(),
            on_change_event: 0,
            on_commit_event: 0,
            background: p.surface_alt,
            border: p.border,
            border_focus: p.accent,
            text_color: p.text,
            placeholder_color: p.text_muted,
            caret_color: p.accent,
            selection_bg: p.selection,
            radius: BorderRadius::all(4.0),
        }
    }

    /// UTF-16 length of the buffer (used by caret math + DirectWrite).
    pub fn text_utf16_len(&self) -> u32 {
        self.text.get().encode_utf16().count() as u32
    }

    /// Insert `s` at the caret, replacing any selection. Updates the signal
    /// (which marks dirty), advances the caret past the inserted text, and
    /// clears the selection anchor. No-op when `disabled`.
    pub fn insert(&mut self, s: &str) {
        if self.disabled {
            return;
        }
        let mut buf = String::from(self.text.get().as_str());
        let (sel_start_u16, sel_end_u16) = self.selection_range();
        let sel_start_byte = utf16_to_byte(&buf, sel_start_u16);
        let sel_end_byte = utf16_to_byte(&buf, sel_end_u16);
        buf.replace_range(sel_start_byte..sel_end_byte, s);
        let inserted_u16 = s.encode_utf16().count() as u32;
        let new_caret = sel_start_u16 + inserted_u16;
        let _ = self.text.set(SmolStr::new(&buf));
        self.caret = new_caret;
        self.selection_anchor = new_caret;
    }

    /// Backspace at caret. If a selection exists, deletes the selection.
    pub fn backspace(&mut self) {
        if self.disabled {
            return;
        }
        if self.caret != self.selection_anchor {
            self.insert("");
            return;
        }
        if self.caret == 0 {
            return;
        }
        let mut buf = String::from(self.text.get().as_str());
        let end_byte = utf16_to_byte(&buf, self.caret);
        let start_byte = utf16_to_byte(&buf, self.caret - 1);
        buf.replace_range(start_byte..end_byte, "");
        let _ = self.text.set(SmolStr::new(&buf));
        self.caret -= 1;
        self.selection_anchor = self.caret;
    }

    /// Move the caret to `pos` (clamped). When `extend` is true the
    /// selection anchor stays put; otherwise both jump together.
    pub fn move_caret(&mut self, pos: u32, extend: bool) {
        let len = self.text_utf16_len();
        let clamped = pos.min(len);
        self.caret = clamped;
        if !extend {
            self.selection_anchor = clamped;
        }
    }

    /// Returns `(start, end)` UTF-16 ranges, normalised so start ≤ end.
    pub fn selection_range(&self) -> (u32, u32) {
        if self.caret <= self.selection_anchor {
            (self.caret, self.selection_anchor)
        } else {
            (self.selection_anchor, self.caret)
        }
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.selection_anchor
    }

    /// Set focus state; callers usually wire this to platform focus events.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Begin / update an IME composition string. The renderer underlines
    /// the composition; `commit_ime` accepts the final text into the buffer.
    pub fn set_ime_composition(&mut self, s: impl Into<SmolStr>) {
        self.ime_composition = s.into();
    }

    pub fn commit_ime(&mut self) {
        if self.ime_composition.is_empty() {
            return;
        }
        let s = self.ime_composition.clone();
        self.ime_composition = SmolStr::default();
        self.insert(s.as_str());
    }

    /// Push a commit event (typically wired to Enter keypress) carrying the
    /// final text snapshot. Returns `true` when an event was pushed.
    pub fn emit_commit<F: FnMut(u32, &str)>(&self, mut sink: F) -> bool {
        if self.on_commit_event == 0 {
            return false;
        }
        sink(self.on_commit_event, self.text.get().as_str());
        true
    }
}

/// Convert a UTF-16 codeunit index to a byte index in `s`. Handles surrogate
/// pairs correctly. Out-of-range indices clamp to `s.len()`.
fn utf16_to_byte(s: &str, u16_idx: u32) -> usize {
    let mut u16_count = 0_u32;
    for (byte_idx, ch) in s.char_indices() {
        if u16_count >= u16_idx {
            return byte_idx;
        }
        u16_count += ch.len_utf16() as u32;
    }
    s.len()
}

impl LayoutSource for Input {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.width),
            height: Length::Px(self.height),
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_insert_advances_caret_and_marks_dirty() {
        let mut i = Input::new("Search…");
        i.text.clear_dirty();
        i.insert("hi");
        assert_eq!(i.text.get().as_str(), "hi");
        assert_eq!(i.caret, 2);
        assert!(i.text.is_dirty());
    }

    #[test]
    fn input_backspace_removes_one_codeunit() {
        let mut i = Input::new("");
        i.insert("hi");
        i.backspace();
        assert_eq!(i.text.get().as_str(), "h");
        assert_eq!(i.caret, 1);
    }

    #[test]
    fn input_backspace_with_selection_deletes_selection() {
        let mut i = Input::new("");
        i.insert("hello");
        i.move_caret(0, false);
        i.move_caret(3, true); // select first 3 chars
        assert!(i.has_selection());
        i.backspace();
        assert_eq!(i.text.get().as_str(), "lo");
    }

    #[test]
    fn input_move_caret_clamps_to_text_length() {
        let mut i = Input::new("");
        i.insert("ab");
        i.move_caret(99, false);
        assert_eq!(i.caret, 2);
    }

    #[test]
    fn input_disabled_insert_is_noop() {
        let mut i = Input::new("");
        i.disabled = true;
        i.insert("x");
        assert_eq!(i.text.get().as_str(), "");
    }

    #[test]
    fn input_commit_ime_inserts_then_clears_composition() {
        let mut i = Input::new("");
        i.set_ime_composition("你好");
        assert_eq!(i.ime_composition.as_str(), "你好");
        i.commit_ime();
        assert_eq!(i.text.get().as_str(), "你好");
        assert!(i.ime_composition.is_empty());
    }

    #[test]
    fn input_emit_commit_pushes_with_text_snapshot() {
        let mut i = Input::new("");
        i.on_commit_event = 9;
        i.insert("done");
        let mut got_id = 0u32;
        let mut got_s = String::new();
        let pushed = i.emit_commit(|id, s| {
            got_id = id;
            got_s = s.to_string();
        });
        assert!(pushed);
        assert_eq!(got_id, 9);
        assert_eq!(got_s, "done");
    }
}
