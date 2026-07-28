//! EncryptionCard — settings panel section for None / DPAPI / Passphrase.
//!
//! Visual spec: `encryption_card.snap.md`. Composition lands when widget-
//! library ships Input (T-017) for the passphrase field and the 3-cell
//! mode-grid composes from existing IconButton + Container primitives
//! (currently shipped).
//!
//! Runtime status: selected-stack complete. The visible encryption row and
//! passphrase capture flow are handled by the D2D settings renderer and shell
//! dispatch path; this module retains the legacy widget-tree contract and the
//! wire enum that backs persisted vault settings.

use bentodesk_style::{Color, Length};
use bentodesk_tree::{NodeId, TreeError};
use bentodesk_widget::{Dropdown, DropdownOption, Input, TextNode, WidgetNode};
use serde::{Deserialize, Serialize};

use crate::business::settings::events as evt;
use crate::state::AppState;

use super::default_card_chrome;

/// Build the EncryptionCard chrome only (back-compat). Use [`mount`] to add
/// the rich body to the AppState tree.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_card_chrome())
}

/// Mount the EncryptionCard subtree under `parent`. Composition:
/// - Title text
/// - Mode dropdown (None / Dpapi / Passphrase) wired to
///   `evt::ENCRYPTION_MODE_CHANGE`
/// - Passphrase Input (only meaningful when mode == Passphrase) wired to
///   `evt::ENCRYPTION_PASSPHRASE_COMMIT`
pub fn mount(app: &mut AppState, parent: NodeId) -> Result<NodeId, TreeError> {
    let card_id = app.add_child(
        parent,
        "settings_encryption_card",
        WidgetNode::Container(default_card_chrome()),
    )?;

    let title = TextNode {
        content: std::borrow::Cow::Borrowed("Encryption"),
        id: None,
        font_size_pt: 13.0,
        font_weight: 500,
        line_height: 1.4,
        color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
        width: Length::Auto,
        height: Length::Px(20.0),
    };
    let _ = app.add_child(card_id, "title", WidgetNode::Text(title));

    let current_mode = read_encryption_mode();

    let options = [
        DropdownOption::new("None", 1),
        DropdownOption::new("DPAPI", 2),
        DropdownOption::new("Passphrase", 3),
    ];
    let mut dd = Dropdown::new(options, evt::ENCRYPTION_MODE_CHANGE);
    dd.selected_value = match current_mode {
        EncryptionMode::None => 1,
        EncryptionMode::Dpapi => 2,
        EncryptionMode::Passphrase => 3,
    };
    let _ = app.add_child(card_id, "mode_dropdown", WidgetNode::Dropdown(dd));

    let mut passphrase = Input::new("Enter passphrase…");
    passphrase.on_commit_event = evt::ENCRYPTION_PASSPHRASE_COMMIT;
    passphrase.disabled = !matches!(current_mode, EncryptionMode::Passphrase);
    let _ = app.add_child(card_id, "passphrase_input", WidgetNode::Input(passphrase));

    Ok(card_id)
}

/// Read the persisted encryption mode. Defaults to `None` when the setting
/// is missing or the vault is unavailable.
fn read_encryption_mode() -> EncryptionMode {
    use bentodesk_backend::config_vault::{SettingValue, Vault};
    match Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(v) => match v.get_setting("encryption.mode") {
                Some(SettingValue::Str(s)) => match s.as_str() {
                    "Dpapi" => EncryptionMode::Dpapi,
                    "Passphrase" => EncryptionMode::Passphrase,
                    _ => EncryptionMode::None,
                },
                _ => EncryptionMode::None,
            },
            Err(_) => {
                tracing::warn!(target: "bentodesk::vault", "EncryptionCard read: vault mutex poisoned; defaulting to None");
                EncryptionMode::None
            }
        },
        None => {
            tracing::debug!(target: "bentodesk::vault", "EncryptionCard read: vault not initialised; defaulting to None");
            EncryptionMode::None
        }
    }
}

/// Encryption mode picker variants. Mirrors the 1.x
/// `services/configVault::EncryptionMode` shape (string union "None" /
/// "Dpapi" / "Passphrase") so the dispatcher Command + `bentodesk-backend`
/// surface speak one closed enum instead of stringly-typed values.
///
/// `serde` derive per the ΔB ruling — preserves the v2.x scripting surface
/// at zero runtime cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncryptionMode {
    /// Plaintext settings.json — backwards-compatible default.
    None,
    /// Per-user transparent encryption via Win32 DPAPI. No passphrase.
    Dpapi,
    /// AES-256-GCM + Argon2id — survives machine migrations. Requires
    /// passphrase entry, validated via probe roundtrip before persisting.
    Passphrase,
}

impl EncryptionMode {
    /// Stable wire-format string — kept identical to the 1.x JSON
    /// `settings.encryption.mode` value so existing settings.json files
    /// load without migration.
    pub const fn as_wire(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Dpapi => "Dpapi",
            Self::Passphrase => "Passphrase",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_round_trips_via_serde_json() {
        let mode = EncryptionMode::Passphrase;
        let json = serde_json::to_string(&mode).expect("serialize");
        // serde's default representation for unit variants is the bare
        // string — kept here so the wire format stays compatible with 1.x.
        assert_eq!(json, "\"Passphrase\"");
        let back: EncryptionMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, mode);
    }

    #[test]
    fn as_wire_matches_serde_string() {
        assert_eq!(EncryptionMode::None.as_wire(), "None");
        assert_eq!(EncryptionMode::Dpapi.as_wire(), "Dpapi");
        assert_eq!(EncryptionMode::Passphrase.as_wire(), "Passphrase");
    }
}
