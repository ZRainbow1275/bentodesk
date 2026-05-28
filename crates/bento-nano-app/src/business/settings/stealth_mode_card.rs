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
    ///
    /// 1:1 with Tauri `StealthModeCard.tsx::deriveLevel` (`:27-31`):
    /// `last_error && !applied → failed`; else `retry_count > 0 → pending`;
    /// else `applied`.
    pub const fn derive(applied: bool, retry_count: u32, has_last_error: bool) -> Self {
        if has_last_error && !applied {
            Self::Failed
        } else if retry_count > 0 {
            Self::Pending
        } else {
            Self::Applied
        }
    }

    /// M1e — single entry point mapping a live
    /// `bento_nano_backend::stealth::StealthStatus` snapshot to the pill
    /// bucket. Panic-free: reads `last_error.is_some()` via `Option::is_some`,
    /// never `.unwrap()`. The renderer and the shell hit-tester both call this
    /// so the conditional rows stay consistent.
    pub fn from_status(s: &bento_nano_backend::stealth::StealthStatus) -> Self {
        Self::derive(s.applied, s.retry_count, s.last_error.is_some())
    }

    /// M1e — the i18n string id for this pill's label (zh/en mirrored at
    /// 158/159/160). Keeps the label-selection logic in the lib so render.rs
    /// stays a thin painter.
    pub const fn label_id(self) -> bento_nano_style::StringId {
        match self {
            Self::Applied => bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_APPLIED,
            Self::Pending => bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_PENDING,
            Self::Failed => bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_FAILED,
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

    // ── M1e — from_status maps the live backend struct (3 combos) ───────

    use bento_nano_backend::stealth::StealthStatus;

    fn status(applied: bool, retry_count: u32, last_error: Option<&str>) -> StealthStatus {
        StealthStatus {
            applied,
            last_error: last_error.map(|s| s.to_string()),
            retry_count,
            schema_version: smol_str::SmolStr::new_static("2"),
            mirror_healthy: true,
        }
    }

    #[test]
    fn from_status_applied_when_clean() {
        let s = status(true, 0, None);
        assert_eq!(StatusLevel::from_status(&s), StatusLevel::Applied);
        assert_eq!(
            StatusLevel::from_status(&s).label_id(),
            bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_APPLIED
        );
    }

    #[test]
    fn from_status_pending_when_retry_pressure() {
        let s = status(true, 2, None);
        assert_eq!(StatusLevel::from_status(&s), StatusLevel::Pending);
        assert_eq!(
            StatusLevel::from_status(&s).label_id(),
            bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_PENDING
        );
    }

    #[test]
    fn from_status_failed_when_error_and_unapplied() {
        let s = status(false, 0, Some("GetLastError=5"));
        assert_eq!(StatusLevel::from_status(&s), StatusLevel::Failed);
        assert_eq!(
            StatusLevel::from_status(&s).label_id(),
            bento_nano_style::i18n_zh_cn::ids::STEALTH_STATUS_FAILED
        );
    }

    #[test]
    fn from_status_never_panics_on_some_error_with_applied() {
        // last_error.is_some() but applied=true → still Applied (no unwrap).
        let s = status(true, 0, Some("transient"));
        assert_eq!(StatusLevel::from_status(&s), StatusLevel::Applied);
    }
}
