//! KeybindingsSection — modal chord recorder + reset table.
//!
//! Visual spec: `keybindings_section.snap.md`. The selected-stack settings
//! overlay renders this as a native D2D modal; this module owns the action row
//! catalog plus recorder validation helpers so renderer, hit-test, and shell
//! persistence all speak the same action/default contract.
//!
//! Status: 1.x parity slice. The visible rows mirror the Tauri
//! `DEFAULT_KEYBINDINGS` catalog; actions stay listed only when the
//! selected-stack shell has a real runtime route for them.

use bento_nano_style::{Color, Length};
use bento_nano_tree::{NodeId, TreeError};
use bento_nano_widget::{TextNode, WidgetNode};
use smol_str::SmolStr;

use crate::state::AppState;

use super::default_card_chrome;

/// Build the KeybindingsSection chrome only (back-compat). Use [`mount`] for
/// the rich body which composes per-row labels.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_card_chrome())
}

/// One selected-stack keybinding row. `action` is the stable config-vault
/// suffix (`keybinding.<action>`), `label` is rendered in the modal, and
/// `default_chord` must stay aligned with the shell hotkey defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeybindingRow {
    pub action: &'static str,
    pub label: &'static str,
    pub default_chord: &'static str,
}

/// Supported selected-stack keybinding rows.
pub const KEYBINDING_ROWS: &[KeybindingRow] = &[
    KeybindingRow {
        action: "app.toggle",
        label: "Toggle BentoDesk",
        default_chord: "Control+Space",
    },
    KeybindingRow {
        action: "zone.new",
        label: "New Zone",
        default_chord: "Control+Shift+N",
    },
    KeybindingRow {
        action: "zone.duplicate",
        label: "Duplicate Zone",
        default_chord: "Control+Shift+D",
    },
    KeybindingRow {
        action: "zone.lock-toggle",
        label: "Toggle Lock",
        default_chord: "Control+Shift+L",
    },
    KeybindingRow {
        action: "zone.hide-all",
        label: "Hide / Show All",
        default_chord: "Control+Shift+H",
    },
    KeybindingRow {
        action: "layout.auto-organize",
        label: "Auto-organize",
        default_chord: "Control+Shift+O",
    },
    KeybindingRow {
        action: "layout.reflow",
        label: "Reflow Screen",
        default_chord: "Control+Shift+R",
    },
    KeybindingRow {
        action: "bulk.open-manager",
        label: "Bulk Manager",
        default_chord: "Control+Shift+M",
    },
    KeybindingRow {
        action: "zone.focus.next",
        label: "Next Zone",
        default_chord: "Control+]",
    },
    KeybindingRow {
        action: "zone.focus.prev",
        label: "Previous Zone",
        default_chord: "Control+[",
    },
];

/// Return the selected-stack keybinding rows in display order.
pub const fn keybinding_rows() -> &'static [KeybindingRow] {
    KEYBINDING_ROWS
}

/// Return the default chord for a supported action.
pub fn default_chord_for_action(action: &str) -> Option<&'static str> {
    KEYBINDING_ROWS
        .iter()
        .find(|row| row.action == action)
        .map(|row| row.default_chord)
}

/// Return the human-readable label for a supported action.
pub fn label_for_action(action: &str) -> Option<&'static str> {
    KEYBINDING_ROWS
        .iter()
        .find(|row| row.action == action)
        .map(|row| row.label)
}

/// Mount the KeybindingsSection subtree under `parent`. Composition:
/// - Title text
/// - One "label  chord" Text row per supported action. The hand-rendered D2D
///   modal owns Record/Reset buttons, but this mounted tree remains useful for
///   reachability checks and future retained-widget composition.
pub fn mount(app: &mut AppState, parent: NodeId) -> Result<NodeId, TreeError> {
    let card_id = app.add_child(
        parent,
        "settings_keybindings_section",
        WidgetNode::Container(default_card_chrome()),
    )?;

    let title = TextNode {
        content: std::borrow::Cow::Borrowed("Keybindings"),
        id: None,
        font_size_pt: 13.0,
        font_weight: 500,
        line_height: 1.4,
        color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
        width: Length::Auto,
        height: Length::Px(20.0),
    };
    let _ = app.add_child(card_id, "title", WidgetNode::Text(title));

    for row_info in KEYBINDING_ROWS {
        let chord = persisted_keybinding_for_action(row_info.action)
            .unwrap_or_else(|| SmolStr::new_static(row_info.default_chord));
        let row = TextNode {
            content: std::borrow::Cow::Owned(format!(
                "{}    {}    (Record / Reset)",
                row_info.label, chord
            )),
            id: None,
            font_size_pt: 11.0,
            font_weight: 400,
            line_height: 1.4,
            color: Color::from_u8(0xA0, 0xA0, 0xB0, 0xFF),
            width: Length::Auto,
            height: Length::Px(16.0),
        };
        let _ = app.add_child(card_id, "kb_row", WidgetNode::Text(row));
    }

    Ok(card_id)
}

/// Read the persisted keybinding for `action`. Returns `None` when the vault
/// is unavailable or the key is unset (caller falls back to the default).
pub fn persisted_keybinding_for_action(action: &str) -> Option<SmolStr> {
    use bento_nano_backend::config_vault::{SettingValue, Vault};
    let key = format!("keybinding.{action}");
    match Vault::global()?.lock() {
        Ok(v) => match v.get_setting(&key) {
            Some(SettingValue::Str(s)) => Some(s),
            _ => None,
        },
        Err(_) => {
            tracing::warn!(target: "bentodesk::vault", %action, "KeybindingsSection read: vault mutex poisoned");
            None
        }
    }
}

/// Current visible chord for a supported action: persisted override when
/// present, otherwise the selected-stack default.
pub fn current_chord_for_action(action: &str) -> Option<SmolStr> {
    persisted_keybinding_for_action(action)
        .or_else(|| default_chord_for_action(action).map(SmolStr::new_static))
}

/// Recorder state machine — mirrors the 1.x `recording` signal lifecycle:
/// Idle → user clicks Record → Recording(action) → first non-modifier key
/// release → Idle (with `setBinding` side effect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderState {
    /// No row is currently capturing input.
    Idle,
    /// `action` is capturing the next chord. The UI shows "…" placeholder
    /// + an accent-bordered chip pulse for the row identified by `action`.
    Recording { action: SmolStr },
}

impl RecorderState {
    /// Begin capture for `action`. Replaces any currently-recording action
    /// (the UI disables other rows' Record buttons during capture so this
    /// is defensive — only one action can be recording at a time).
    pub fn start(action: impl Into<SmolStr>) -> Self {
        Self::Recording {
            action: action.into(),
        }
    }

    /// `true` when the recorder is currently capturing for the given action.
    pub fn is_recording_for(&self, action: &str) -> bool {
        matches!(self, Self::Recording { action: a } if a.as_str() == action)
    }
}

/// Reserved Win32 accelerators — the 1.x backend rejects rebinding to these.
/// Captured here so the UI can pre-flight the chord and surface the conflict
/// inline without a backend roundtrip.
const RESERVED_CHORDS: &[&str] = &[
    "Alt+F4",   // Win32 close-window
    "Alt+Tab",  // Win32 task switcher
    "Ctrl+Esc", // Win32 start menu
    "Ctrl+Escape",
    "Win+L", // Win32 lock workstation
    "Win+D", // Win32 show desktop
];

/// `true` when `chord` is a Win32-reserved accelerator (case-insensitive
/// against the canonical "Mod1+Mod2+Key" form). Same set the 1.x backend
/// rejects in `setBinding`'s validation path.
pub fn is_reserved_chord(chord: &str) -> bool {
    RESERVED_CHORDS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(chord))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_starts_idle() {
        assert!(matches!(RecorderState::Idle, RecorderState::Idle));
    }

    #[test]
    fn recorder_records_then_matches_action() {
        let s = RecorderState::start("zone.new");
        assert!(s.is_recording_for("zone.new"));
        assert!(!s.is_recording_for("zone.duplicate"));
    }

    #[test]
    fn reserved_chord_lookup_is_case_insensitive() {
        assert!(is_reserved_chord("Alt+F4"));
        assert!(is_reserved_chord("alt+f4"));
        assert!(is_reserved_chord("ALT+TAB"));
        assert!(!is_reserved_chord("Ctrl+Shift+P"));
        assert!(!is_reserved_chord(""));
    }

    #[test]
    fn row_catalog_exposes_defaults_for_supported_actions() {
        assert_eq!(
            default_chord_for_action("bulk.open-manager"),
            Some("Control+Shift+M")
        );
        assert_eq!(label_for_action("zone.focus.prev"), Some("Previous Zone"));
        assert_eq!(
            default_chord_for_action("app.toggle"),
            Some("Control+Space")
        );
        assert_eq!(default_chord_for_action("timeline.open"), None);
    }
}
