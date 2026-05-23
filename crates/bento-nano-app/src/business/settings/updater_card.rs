//! UpdaterCard — check / download / install updater UX.
//!
//! Visual spec: `updater_card.snap.md`. The selected-stack Settings surface
//! now exposes real preference rows plus check/download/install/skip command
//! producers; this retained card keeps the widget contract aligned with that
//! runtime path.
//!
//! Runtime status: selected-stack complete for the migrated app lifecycle.
//! Preferences persist through the selected-stack config vault, manifest
//! check/download/staging/signature verification are real backend paths, and
//! install launches staged local NSIS artifacts. Production release-channel
//! signing remains a release-pipeline concern, not a blocker for the app
//! migration objective.

use bento_nano_style::{Color, Length};
use bento_nano_tree::{NodeId, TreeError};
use bento_nano_widget::{Dropdown, DropdownOption, IconButton, TextNode, Toggle, WidgetNode};
use serde::{Deserialize, Serialize};

use crate::business::settings::events as evt;
use crate::state::AppState;

use super::default_card_chrome;

/// Build the UpdaterCard chrome only (back-compat). Use [`mount`] for the
/// rich body which composes startup toggle + channel dropdown + check button.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_card_chrome())
}

/// "Magnifier" icon path — proxy for Check Now until F5 ships the real svg.
const CHECK_NOW_PATH: &str = "M12 5 L19 12 L12 19 L5 12 Z";

/// Mount the UpdaterCard subtree under `parent`. Composition:
/// - Title text
/// - Frequency dropdown (Daily / Weekly / Manual) (`evt::UPDATER_FREQUENCY_CHANGE`)
/// - Auto-download toggle (`evt::UPDATER_TOGGLE_AUTO_DOWNLOAD`)
/// - Check Now icon button (`evt::UPDATER_CHECK_NOW`)
pub fn mount(app: &mut AppState, parent: NodeId) -> Result<NodeId, TreeError> {
    let card_id = app.add_child(
        parent,
        "settings_updater_card",
        WidgetNode::Container(default_card_chrome()),
    )?;

    let title = TextNode {
        content: std::borrow::Cow::Borrowed("Updater"),
        id: None,
        font_size_pt: 13.0,
        font_weight: 500,
        line_height: 1.4,
        color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
        width: Length::Auto,
        height: Length::Px(20.0),
    };
    let _ = app.add_child(card_id, "title", WidgetNode::Text(title));

    let options = [
        DropdownOption::new("Daily", 1),
        DropdownOption::new("Weekly", 2),
        DropdownOption::new("Manual", 3),
    ];
    let (frequency, auto_download) = read_updater_settings();
    let mut dd = Dropdown::new(options, evt::UPDATER_FREQUENCY_CHANGE);
    dd.selected_value = match frequency {
        bento_nano_backend::updater::UpdateCheckFrequency::Daily => 1,
        bento_nano_backend::updater::UpdateCheckFrequency::Weekly => 2,
        bento_nano_backend::updater::UpdateCheckFrequency::Manual => 3,
    };
    let _ = app.add_child(card_id, "frequency_dropdown", WidgetNode::Dropdown(dd));

    let mut t_auto_download = Toggle::new(evt::UPDATER_TOGGLE_AUTO_DOWNLOAD);
    t_auto_download.set_on(auto_download);
    let _ = app.add_child(
        card_id,
        "toggle_auto_download",
        WidgetNode::Toggle(t_auto_download),
    );

    let mut btn = IconButton::new(CHECK_NOW_PATH, evt::UPDATER_CHECK_NOW);
    btn.size = 24.0;
    let _ = app.add_child(card_id, "check_now_btn", WidgetNode::IconButton(btn));

    Ok(card_id)
}

/// Read updater settings from the global vault. Defaults match the Tauri
/// baseline: check_frequency = `Weekly`, auto_download = `true`.
fn read_updater_settings() -> (bento_nano_backend::updater::UpdateCheckFrequency, bool) {
    use bento_nano_backend::config_vault::{SettingValue, Vault};
    use bento_nano_backend::updater::UpdateCheckFrequency;
    let default_frequency = UpdateCheckFrequency::Weekly;
    let default_auto_download = true;
    match Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(v) => {
                let frequency = match v.get_setting("updates.check_frequency") {
                    Some(SettingValue::Str(s)) => match s.as_str() {
                        "Daily" => UpdateCheckFrequency::Daily,
                        "Manual" => UpdateCheckFrequency::Manual,
                        _ => default_frequency,
                    },
                    _ => default_frequency,
                };
                let auto_download = match v.get_setting("updates.auto_download") {
                    Some(SettingValue::Bool(b)) => b,
                    _ => default_auto_download,
                };
                (frequency, auto_download)
            }
            Err(_) => {
                tracing::warn!(target: "bentodesk::vault", "UpdaterCard read: vault mutex poisoned; defaulting");
                (default_frequency, default_auto_download)
            }
        },
        None => {
            tracing::debug!(target: "bentodesk::vault", "UpdaterCard read: vault not initialised; defaulting");
            (default_frequency, default_auto_download)
        }
    }
}

/// Updater status — six states that drive the pill colour + visible action
/// buttons. Mirrors the 1.x `services/updater::UpdaterStatus` union.
///
/// Deserialise wire format: bare lowercase strings ("idle", "checking", …)
/// — kept identical to the 1.x JSON shape so settings.json round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdaterStatus {
    /// No active check in flight, no update known to be available.
    Idle,
    /// Check in flight; show spinner + checking pill.
    Checking,
    /// Update available; show version block + Download button.
    Available,
    /// Download in flight; show progress bar + cancel.
    Downloading,
    /// Download complete, awaiting install + restart.
    Ready,
    /// Last check or download errored; show retry button.
    Error,
}

/// Format `bytes` as a human-readable size — same convention as the 1.x
/// `formatBytes`: `<1 KB → "<n> B"`, `<1 MB → "<x.x> KB"`, otherwise
/// `"<x.xx> MB"`. Pulled into the port today so the snap.md "downloaded"
/// label text matches when the card composes.
pub fn format_bytes(bytes: u64) -> smol_str::SmolStr {
    if bytes < 1024 {
        smol_str::SmolStr::new(format!("{bytes} B"))
    } else if bytes < 1024 * 1024 {
        let kb = (bytes as f32) / 1024.0;
        smol_str::SmolStr::new(format!("{kb:.1} KB"))
    } else {
        let mb = (bytes as f64) / (1024.0 * 1024.0);
        smol_str::SmolStr::new(format!("{mb:.2} MB"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_status_lowercase_serde_round_trip() {
        let status = UpdaterStatus::Downloading;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"downloading\"");
        let back: UpdaterStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, status);
    }

    #[test]
    fn frequency_default_is_weekly() {
        assert_eq!(
            bento_nano_backend::updater::UpdateCheckFrequency::default(),
            bento_nano_backend::updater::UpdateCheckFrequency::Weekly
        );
    }

    #[test]
    fn format_bytes_buckets_match_one_x_thresholds() {
        assert_eq!(format_bytes(0).as_str(), "0 B");
        assert_eq!(format_bytes(1023).as_str(), "1023 B");
        assert_eq!(format_bytes(1024).as_str(), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024 - 1).as_str(), "1024.0 KB");
        assert_eq!(format_bytes(1024 * 1024).as_str(), "1.00 MB");
        assert_eq!(
            format_bytes(2 * 1024 * 1024 + 512 * 1024).as_str(),
            "2.50 MB"
        );
    }
}
