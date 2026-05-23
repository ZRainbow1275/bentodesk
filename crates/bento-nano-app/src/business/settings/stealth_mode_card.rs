//! StealthModeCard — R3 stealth status panel.
//!
//! Visual spec: `stealth_mode_card.snap.md`. Composition lands when widget-
//! library ships the Status pill (composable from BentoCard already) and
//! the OneDrive warning panel (Container + Text + Button — all available).
//! Today's blocker is purely waiting on the card-row rhythm shared with the
//! other settings cards.
//!
//! Runtime status: selected-stack complete. The visible stealth storage row
//! and its status feedback are rendered by the D2D settings surface; this
//! module keeps the widget-tree compatibility contract and the 1.x status
//! derivation helper stable for tests and layout consumers.

use bento_nano_style::{Color, Length};
use bento_nano_tree::{NodeId, TreeError};
use bento_nano_widget::{TextNode, Toggle, WidgetNode};

use crate::business::settings::events as evt;
use crate::state::AppState;

use super::default_card_chrome;

/// Build the StealthModeCard chrome only (back-compat). Use [`mount`] for the
/// rich body which composes 3 toggles + status row.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_card_chrome())
}

/// Mount the StealthModeCard subtree under `parent`. Composition:
/// - Title text
/// - Enable toggle (`evt::STEALTH_TOGGLE_ENABLED`)
/// - Hidden-by-default toggle (`evt::STEALTH_TOGGLE_HIDDEN`)
/// - Exclude-from-screen-capture toggle (`evt::STEALTH_TOGGLE_EXCLUDE_CAPTURE`)
///   — the WDA_EXCLUDEFROMCAPTURE wiring lands in F6; F2 only persists the
///   user's preference.
pub fn mount(app: &mut AppState, parent: NodeId) -> Result<NodeId, TreeError> {
    let card_id = app.add_child(
        parent,
        "settings_stealth_mode_card",
        WidgetNode::Container(default_card_chrome()),
    )?;

    let title = TextNode {
        content: std::borrow::Cow::Borrowed("Stealth Mode"),
        id: None,
        font_size_pt: 13.0,
        font_weight: 500,
        line_height: 1.4,
        color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
        width: Length::Auto,
        height: Length::Px(20.0),
    };
    let _ = app.add_child(card_id, "title", WidgetNode::Text(title));

    let (enabled, hidden, exclude_capture) = read_stealth_settings();

    let mut t_enabled = Toggle::new(evt::STEALTH_TOGGLE_ENABLED);
    t_enabled.set_on(enabled);
    let _ = app.add_child(card_id, "toggle_enabled", WidgetNode::Toggle(t_enabled));

    let mut t_hidden = Toggle::new(evt::STEALTH_TOGGLE_HIDDEN);
    t_hidden.set_on(hidden);
    let _ = app.add_child(card_id, "toggle_hidden", WidgetNode::Toggle(t_hidden));

    let mut t_exclude = Toggle::new(evt::STEALTH_TOGGLE_EXCLUDE_CAPTURE);
    t_exclude.set_on(exclude_capture);
    let _ = app.add_child(
        card_id,
        "toggle_exclude_capture",
        WidgetNode::Toggle(t_exclude),
    );

    Ok(card_id)
}

/// Read the three stealth toggle states from the global vault. The master
/// switch defaults to `true` to preserve the existing Tauri hidden-items
/// behaviour; the optional visibility refinements default to `false`.
fn read_stealth_settings() -> (bool, bool, bool) {
    use bento_nano_backend::config_vault::{SettingValue, Vault};
    let default_enabled = true;
    let read_bool = |v: &bento_nano_backend::config_vault::Vault, key: &str| -> bool {
        matches!(v.get_setting(key), Some(SettingValue::Bool(true)))
    };
    match Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(v) => (
                match v.get_setting("stealth.enabled") {
                    Some(SettingValue::Bool(value)) => value,
                    _ => default_enabled,
                },
                read_bool(&v, "stealth.hidden_by_default"),
                read_bool(&v, "stealth.exclude_from_capture"),
            ),
            Err(_) => {
                tracing::warn!(target: "bentodesk::vault", "StealthModeCard read: vault mutex poisoned; defaulting");
                (default_enabled, false, false)
            }
        },
        None => {
            tracing::debug!(target: "bentodesk::vault", "StealthModeCard read: vault not initialised; defaulting");
            (default_enabled, false, false)
        }
    }
}

/// Status pill colour selection — three-way mapping from the
/// `bento-nano-backend::stealth::StealthStatus` shape. Mirrors the 1.x
/// `deriveLevel` helper so the pill colour stays identical when the body
/// composition lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusLevel {
    /// Stealth applied successfully, no retries pending. Pill renders green.
    Applied,
    /// Stealth applied but with retry pressure (transient). Pill amber.
    Pending,
    /// Stealth failed — `last_error` populated AND `applied == false`. Red.
    Failed,
}

impl StatusLevel {
    /// Compute the pill colour bucket from the StealthStatus probe shape:
    /// `(applied, retry_count, has_last_error)`. Tuple input keeps this
    /// callable from a future `bento-nano-backend::stealth::StealthStatus`
    /// without a circular dep on the backend crate.
    pub const fn derive(applied: bool, retry_count: u32, has_last_error: bool) -> Self {
        if has_last_error && !applied {
            Self::Failed
        } else if retry_count > 0 {
            Self::Pending
        } else {
            Self::Applied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applied_with_no_retry_no_error_is_green() {
        assert_eq!(StatusLevel::derive(true, 0, false), StatusLevel::Applied);
    }

    #[test]
    fn retry_pressure_promotes_to_pending() {
        // Even when applied, a non-zero retry count is the 1.x pending tone.
        assert_eq!(StatusLevel::derive(true, 1, false), StatusLevel::Pending);
        // Without applied flag, retry pressure still pending (pre-failure).
        assert_eq!(StatusLevel::derive(false, 1, false), StatusLevel::Pending);
    }

    #[test]
    fn last_error_with_unapplied_is_failure() {
        assert_eq!(StatusLevel::derive(false, 0, true), StatusLevel::Failed);
    }

    #[test]
    fn last_error_with_applied_stays_applied() {
        // Edge case from 1.x deriveLevel — `last_error && applied` means
        // a transient error already recovered; the pill stays green.
        assert_eq!(StatusLevel::derive(true, 0, true), StatusLevel::Applied);
    }
}
