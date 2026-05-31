//! `Dialog` — modal dialog with title + body + OK/Cancel actions.
//!
//! 1.x source: `bentodesk/src/components/shared/PromptModal.tsx`. Solid-JS
//! portal-mounted scrim + centred panel with a single-line text input and
//! a primary / secondary button row. Replaces `window.prompt()` which is
//! unreliable under Tauri's overlay passthrough.
//!
//! Visual fidelity reference: `dialog.snap.md`.
//!
//! # Hosting
//!
//! In 2.0 the dialog visual is rendered into a `WindowKind::Settings`-style
//! HWND when the caller wants modal behaviour (popup-window with caption),
//! or — for simpler in-place prompts — into the Main HWND's overlay layer
//! the way the existing `app::settings_panel` does. This file owns the
//! descriptor + state surface; the chosen host is the caller's decision.
//!
//! # State
//!
//! [`DialogState`] tracks the input value + which action (if any) the
//! user has confirmed. The shell drains the action via
//! [`DialogState::take_action`] each frame; an action stays `None` until
//! the user clicks OK / Cancel or presses Enter / Escape.

use core::fmt;

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::tokens::{PaletteTauri, RadiusTauri, SpacingTauri};
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme as theme;
use smol_str::SmolStr;

/// Action the user took on the dialog. `None` means no action yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    /// User confirmed (clicked OK / pressed Enter). Carries the final
    /// input text.
    Submit(SmolStr),
    /// User cancelled (clicked Cancel / pressed Escape / clicked the
    /// scrim).
    Cancel,
}

impl fmt::Display for DialogAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Submit(t) => write!(f, "Submit({t:?})"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

// -----------------------------------------------------------------------------
// Dialog widget descriptor — the visual chrome.
// -----------------------------------------------------------------------------

/// Modal dialog descriptor — title row + body slot + action button row.
/// Matches the 1.x `PromptModal.css` panel chrome.
#[derive(Debug, Clone)]
pub struct Dialog {
    /// Dialog title — `SmolStr` keeps short headings inline.
    pub title: SmolStr,
    /// Placeholder text shown in the input when empty.
    pub placeholder: SmolStr,
    /// Label for the primary (confirm) action button.
    pub ok_label: SmolStr,
    /// Label for the secondary (cancel) action button.
    pub cancel_label: SmolStr,
    /// Panel background — `palette.surface`.
    pub background: Color,
    /// Title text colour — `palette.text`.
    pub title_color: Color,
    /// Border radius — `12 px` per `PromptModal.css`.
    pub border_radius: BorderRadius,
    /// Inset padding — `20 px` uniform per `PromptModal.css`.
    pub padding: Edges,
    /// Default panel width — `400 px` per 1.x.
    pub width: Length,
    /// Default panel height — `Length::Auto` (sized to title + input + buttons).
    pub height: Length,
}

impl Dialog {
    /// Construct a dialog. `title` is the heading; the other strings default
    /// to the 1.x English fallbacks `"OK" / "Cancel" / ""`.
    pub fn new(title: impl Into<SmolStr>) -> Self {
        let palette = theme::current().palette;
        Self {
            title: title.into(),
            placeholder: SmolStr::default(),
            ok_label: SmolStr::new_static("OK"),
            cancel_label: SmolStr::new_static("Cancel"),
            background: palette.surface,
            title_color: palette.text,
            border_radius: BorderRadius::all(12.0),
            padding: Edges::all(20.0),
            width: Length::Px(400.0),
            height: Length::Auto,
        }
    }

    /// Construct a dialog from Wave B Tauri SSoT tokens.
    ///
    /// Token mapping (Wave A `chrome.md` dialog metrics — denser than popover):
    /// - background ← `surface_dialog` (0.92α — distinct from `surface_expanded` 0.82α)
    /// - title colour ← `text_primary`
    /// - radius ← `expanded` (16)
    /// - padding ← 16 px vertical / 20 px horizontal (Wave A literals)
    pub fn from_tauri_tokens(
        title: impl Into<SmolStr>,
        palette: PaletteTauri,
        radius: RadiusTauri,
        spacing: SpacingTauri,
    ) -> Self {
        Self {
            title: title.into(),
            placeholder: SmolStr::default(),
            ok_label: SmolStr::new_static("OK"),
            cancel_label: SmolStr::new_static("Cancel"),
            background: palette.surface_dialog,
            title_color: palette.text_primary,
            border_radius: BorderRadius::all(radius.expanded),
            padding: Edges {
                top: spacing.lg,
                right: spacing.xl,
                bottom: spacing.lg,
                left: spacing.xl,
            },
            width: Length::Px(400.0),
            height: Length::Auto,
        }
    }

    /// Override the OK button label.
    pub fn with_ok_label(mut self, label: impl Into<SmolStr>) -> Self {
        self.ok_label = label.into();
        self
    }

    /// Override the Cancel button label.
    pub fn with_cancel_label(mut self, label: impl Into<SmolStr>) -> Self {
        self.cancel_label = label.into();
        self
    }

    /// Override the input placeholder.
    pub fn with_placeholder(mut self, placeholder: impl Into<SmolStr>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

impl LayoutSource for Dialog {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Column: title → input → action row.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

// -----------------------------------------------------------------------------
// DialogState — input value + pending-action surface.
// -----------------------------------------------------------------------------

/// User-edited input + the latest action (if any). The shell drains
/// the action via [`take_action`] once per frame; until then the action
/// persists so a missed paint cycle doesn't lose the user's click.
///
/// Defaults to an empty input + no action.
///
/// [`take_action`]: DialogState::take_action
#[derive(Debug, Default)]
pub struct DialogState {
    value: SmolStr,
    action: Option<DialogAction>,
}

impl DialogState {
    /// New empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the input with a default value (1.x `defaultValue` prop).
    pub fn seeded(default_value: impl Into<SmolStr>) -> Self {
        Self {
            value: default_value.into(),
            action: None,
        }
    }

    /// Borrow the current input text.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Replace the input text (called by the shell on each text-input
    /// event from the host HWND).
    pub fn set_value(&mut self, v: impl Into<SmolStr>) {
        self.value = v.into();
    }

    /// User clicked OK / pressed Enter. Records `Submit(value)` so the
    /// next `take_action` returns it.
    pub fn confirm(&mut self) {
        self.action = Some(DialogAction::Submit(self.value.clone()));
    }

    /// User clicked Cancel / pressed Escape / clicked the scrim. Records
    /// `Cancel` so the next `take_action` returns it.
    pub fn cancel(&mut self) {
        self.action = Some(DialogAction::Cancel);
    }

    /// Drain the latest action. Returns `None` when the user hasn't
    /// confirmed/cancelled yet. Subsequent calls without further user
    /// interaction return `None` too — the action is one-shot.
    pub fn take_action(&mut self) -> Option<DialogAction> {
        self.action.take()
    }

    /// Whether an action is pending — diagnostics + UI affordance gating.
    pub fn has_pending_action(&self) -> bool {
        self.action.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_default_chrome_uses_palette_surface() {
        let d = Dialog::new("Rename Zone");
        let palette = theme::current().palette;
        assert_eq!(d.background, palette.surface);
        assert_eq!(d.title_color, palette.text);
        assert_eq!(d.border_radius.top_left, 12.0);
        assert_eq!(d.width, Length::Px(400.0));
    }

    #[test]
    fn dialog_with_overrides_replaces_labels() {
        let d = Dialog::new("Do thing")
            .with_ok_label("Sure")
            .with_cancel_label("Nope")
            .with_placeholder("Type here");
        assert_eq!(d.ok_label, SmolStr::new_static("Sure"));
        assert_eq!(d.cancel_label, SmolStr::new_static("Nope"));
        assert_eq!(d.placeholder, SmolStr::new_static("Type here"));
    }

    #[test]
    fn dialog_state_seeded_starts_with_default_value() {
        let s = DialogState::seeded("My Zone");
        assert_eq!(s.value(), "My Zone");
        assert!(!s.has_pending_action());
    }

    #[test]
    fn dialog_state_confirm_records_submit_with_current_value() {
        let mut s = DialogState::new();
        s.set_value("hello");
        s.confirm();
        assert!(s.has_pending_action());
        assert_eq!(
            s.take_action(),
            Some(DialogAction::Submit(SmolStr::new_static("hello")))
        );
        // One-shot — second take returns None.
        assert!(s.take_action().is_none());
    }

    #[test]
    fn dialog_state_cancel_records_cancel() {
        let mut s = DialogState::seeded("ignored");
        s.cancel();
        assert_eq!(s.take_action(), Some(DialogAction::Cancel));
    }

    #[test]
    fn dialog_state_take_action_clears_pending_flag() {
        let mut s = DialogState::new();
        s.confirm();
        let _ = s.take_action();
        assert!(!s.has_pending_action());
    }

    #[test]
    // The `surface_dialog.a > surface_expanded.a` assertion below compares
    // const token alphas (Wave A chrome.md invariant), so clippy sees a
    // constant value — that is the intended compile-time guard.
    #[allow(clippy::assertions_on_constants)]
    fn dialog_from_tauri_tokens_consumes_wave_b_ssot() {
        use bento_nano_style::tokens as style_tokens;
        let d = Dialog::from_tauri_tokens(
            "删除区域?",
            style_tokens::PALETTE_DARK,
            style_tokens::RADIUS,
            style_tokens::SPACING,
        );
        // Wave A chrome.md: dialog uses the denser `surface_dialog` (0.92α).
        assert_eq!(d.background, style_tokens::PALETTE_DARK.surface_dialog);
        assert!(
            style_tokens::PALETTE_DARK.surface_dialog.a > style_tokens::PALETTE_DARK.surface_expanded.a,
            "dialog alpha must be denser than expanded per Wave A chrome.md"
        );
        assert_eq!(d.title_color, style_tokens::PALETTE_DARK.text_primary);
        assert_eq!(d.border_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
        assert_eq!(d.padding.top, style_tokens::SPACING.lg);
        assert_eq!(d.padding.left, style_tokens::SPACING.xl);
    }
}
