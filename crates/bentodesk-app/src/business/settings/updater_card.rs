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

use bentodesk_style::{Color, Length};
use bentodesk_tree::{NodeId, TreeError};
use bentodesk_widget::{Dropdown, DropdownOption, IconButton, TextNode, Toggle, WidgetNode};
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
        bentodesk_backend::updater::UpdateCheckFrequency::Daily => 1,
        bentodesk_backend::updater::UpdateCheckFrequency::Weekly => 2,
        bentodesk_backend::updater::UpdateCheckFrequency::Manual => 3,
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
fn read_updater_settings() -> (bentodesk_backend::updater::UpdateCheckFrequency, bool) {
    use bentodesk_backend::config_vault::{SettingValue, Vault};
    use bentodesk_backend::updater::UpdateCheckFrequency;
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

// ── M1f 2026-05-29 — inline Updater §8 paint/hit logic ──────────────────
//
// The visible Updater card is painted by `Renderer::draw_settings_panel`
// from the live `AppState::settings_updater_status` snapshot (9 variants),
// NOT from the widget-tree `mount` above. The pure status→pill / button-
// visibility / progress-fraction helpers below keep that drawing logic in
// the lib crate (testable, panic-free) so render.rs + the shell hit-tester
// stay thin painters. 1:1 with Tauri `UpdaterCard.tsx` (`statusPillLabel`
// + the three `<Show when=…>` action gates).

use crate::state::SettingsUpdaterStatus;

/// M1f — pill colour bucket for the Updater status row. Mirrors the Tauri
/// `.updater-status-{state}` CSS tints: idle/up-to-date → green, checking →
/// muted/grey, available/downloading/installing → blue, ready → green,
/// skipped → grey, error → red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdaterPillKind {
    /// No update activity / already on the latest build. Green.
    UpToDate,
    /// A check or background work is in flight. Muted grey.
    Busy,
    /// An update is available, downloading, or installing. Blue.
    Active,
    /// A staged update is ready to install + restart. Green.
    Ready,
    /// The current version was skipped by the user. Grey.
    Skipped,
    /// The last check / download / install errored. Red.
    Error,
}

impl UpdaterPillKind {
    /// M1f — map a live [`SettingsUpdaterStatus`] to its pill colour bucket.
    /// Panic-free total match over all 9 variants.
    pub const fn from_status(status: &SettingsUpdaterStatus) -> Self {
        match status {
            SettingsUpdaterStatus::Idle | SettingsUpdaterStatus::UpToDate { .. } => Self::UpToDate,
            SettingsUpdaterStatus::Checking => Self::Busy,
            SettingsUpdaterStatus::Available { .. }
            | SettingsUpdaterStatus::Downloading { .. }
            | SettingsUpdaterStatus::Installing { .. } => Self::Active,
            SettingsUpdaterStatus::Ready { .. } => Self::Ready,
            SettingsUpdaterStatus::Skipped { .. } => Self::Skipped,
            SettingsUpdaterStatus::Error(_) => Self::Error,
        }
    }
}

/// M1f — i18n string id for the status pill label of a given updater status.
/// Each of the 9 variants maps to one of the appended ids 172..181. Returns
/// the id (the renderer calls `bentodesk_style::t(..)`).
pub const fn updater_status_label_id(status: &SettingsUpdaterStatus) -> bentodesk_style::StringId {
    use bentodesk_style::i18n_zh_cn::ids;
    match status {
        SettingsUpdaterStatus::Idle => ids::UPDATER_STATUS_IDLE,
        SettingsUpdaterStatus::Checking => ids::UPDATER_STATUS_CHECKING,
        SettingsUpdaterStatus::UpToDate { .. } => ids::UPDATER_STATUS_UP_TO_DATE,
        SettingsUpdaterStatus::Available { .. } => ids::UPDATER_STATUS_AVAILABLE,
        SettingsUpdaterStatus::Downloading { .. } => ids::UPDATER_STATUS_DOWNLOADING,
        SettingsUpdaterStatus::Ready { .. } => ids::UPDATER_STATUS_READY,
        SettingsUpdaterStatus::Installing { .. } => ids::UPDATER_STATUS_INSTALLING,
        SettingsUpdaterStatus::Skipped { .. } => ids::UPDATER_STATUS_SKIPPED,
        SettingsUpdaterStatus::Error(_) => ids::UPDATER_STATUS_ERROR,
    }
}

/// M1f — which conditional version SmolStr (if any) the card shows below the
/// status row. Available/Ready/Installing/Skipped carry a version; the other
/// states show no version block. Borrows the already-allocated SmolStr so
/// paint stays alloc-free (§10).
pub fn updater_visible_version(status: &SettingsUpdaterStatus) -> Option<&smol_str::SmolStr> {
    match status {
        SettingsUpdaterStatus::Available { version }
        | SettingsUpdaterStatus::Ready { version }
        | SettingsUpdaterStatus::Installing { version }
        | SettingsUpdaterStatus::Skipped { version } => Some(version),
        _ => None,
    }
}

/// M1f — download progress fraction in `0.0..=1.0`, or `None` when the total
/// size is unknown (indeterminate bar). Only meaningful while
/// `Downloading`; every other state returns `None` (no bar painted).
///
/// Panic-free: guards `total_bytes == 0` (→ `None`, never a divide-by-zero)
/// and clamps the ratio to `1.0` so a late chunk that overshoots the
/// advertised total can't paint a fill wider than the track.
pub fn updater_progress_fraction(status: &SettingsUpdaterStatus) -> Option<f32> {
    match status {
        SettingsUpdaterStatus::Downloading {
            chunk_len,
            total_bytes,
        } => match total_bytes {
            Some(total) if *total > 0 => {
                let frac = (*chunk_len as f64 / *total as f64) as f32;
                Some(frac.clamp(0.0, 1.0))
            }
            // total unknown (or zero) → indeterminate bar.
            _ => None,
        },
        _ => None,
    }
}

/// M1f — true when the `下载 / Download` action button should be visible
/// (status `Available`). 1:1 with Tauri `<Show when={status()==="available"}>`.
pub const fn updater_show_download(status: &SettingsUpdaterStatus) -> bool {
    matches!(status, SettingsUpdaterStatus::Available { .. })
}

/// M1f — true when the `安装并重启 / Install and restart` action button should
/// be visible (status `Ready`). 1:1 with Tauri `<Show when={status()==="ready"}>`.
pub const fn updater_show_install(status: &SettingsUpdaterStatus) -> bool {
    matches!(status, SettingsUpdaterStatus::Ready { .. })
}

/// M1f — true when the `跳过此版本 / Skip this version` action button should be
/// visible. Tauri shows it for `available`; native additionally allows it for
/// `Ready` (a staged-but-not-installed update can still be skipped — the
/// shell's `version_for_skip` already supports both). Reuses the existing
/// [`SettingsUpdaterStatus::can_skip_update`] gate so paint + dispatch agree.
pub const fn updater_show_skip(status: &SettingsUpdaterStatus) -> bool {
    status.can_skip_update()
}

/// M1f — collapse a live [`SettingsUpdaterStatus`] to the
/// [`crate::settings_panel::UpdaterHeightKind`] discriminant that drives the
/// card's dynamic body height. The middle block (version / progress / error)
/// is mutually exclusive by status family, so the height only needs this 4-way
/// classification (keeping `SettingsBodyFlags` `Copy` + SmolStr-free). The
/// renderer + the shell scroll-clamp both call this so paint and clamp agree.
pub const fn updater_height_kind(
    status: &SettingsUpdaterStatus,
) -> crate::settings_panel::UpdaterHeightKind {
    use crate::settings_panel::UpdaterHeightKind as K;
    match status {
        SettingsUpdaterStatus::Available { .. }
        | SettingsUpdaterStatus::Ready { .. }
        | SettingsUpdaterStatus::Installing { .. }
        | SettingsUpdaterStatus::Skipped { .. } => K::Versioned,
        SettingsUpdaterStatus::Downloading { .. } => K::Downloading,
        SettingsUpdaterStatus::Error(_) => K::Error,
        SettingsUpdaterStatus::Idle
        | SettingsUpdaterStatus::Checking
        | SettingsUpdaterStatus::UpToDate { .. } => K::StatusOnly,
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
            bentodesk_backend::updater::UpdateCheckFrequency::default(),
            bentodesk_backend::updater::UpdateCheckFrequency::Weekly
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

    // ── M1f — inline Updater §8 paint/hit logic ─────────────────────────

    use crate::state::SettingsUpdaterStatus as S;
    use smol_str::SmolStr;

    fn ver() -> SmolStr {
        SmolStr::new_static("1.4.0")
    }

    #[test]
    fn pill_kind_covers_every_status_variant() {
        assert_eq!(
            UpdaterPillKind::from_status(&S::Idle),
            UpdaterPillKind::UpToDate
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::UpToDate {
                current_version: ver()
            }),
            UpdaterPillKind::UpToDate
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Checking),
            UpdaterPillKind::Busy
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Available { version: ver() }),
            UpdaterPillKind::Active
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Downloading {
                chunk_len: 1,
                total_bytes: Some(2)
            }),
            UpdaterPillKind::Active
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Installing { version: ver() }),
            UpdaterPillKind::Active
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Ready { version: ver() }),
            UpdaterPillKind::Ready
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Skipped { version: ver() }),
            UpdaterPillKind::Skipped
        );
        assert_eq!(
            UpdaterPillKind::from_status(&S::Error(SmolStr::new_static("boom"))),
            UpdaterPillKind::Error
        );
    }

    #[test]
    fn status_label_id_maps_each_variant_to_a_distinct_appended_id() {
        use bentodesk_style::i18n_zh_cn::ids;
        assert_eq!(updater_status_label_id(&S::Idle), ids::UPDATER_STATUS_IDLE);
        assert_eq!(
            updater_status_label_id(&S::Checking),
            ids::UPDATER_STATUS_CHECKING
        );
        assert_eq!(
            updater_status_label_id(&S::UpToDate {
                current_version: ver()
            }),
            ids::UPDATER_STATUS_UP_TO_DATE
        );
        assert_eq!(
            updater_status_label_id(&S::Available { version: ver() }),
            ids::UPDATER_STATUS_AVAILABLE
        );
        assert_eq!(
            updater_status_label_id(&S::Downloading {
                chunk_len: 0,
                total_bytes: None
            }),
            ids::UPDATER_STATUS_DOWNLOADING
        );
        assert_eq!(
            updater_status_label_id(&S::Ready { version: ver() }),
            ids::UPDATER_STATUS_READY
        );
        assert_eq!(
            updater_status_label_id(&S::Installing { version: ver() }),
            ids::UPDATER_STATUS_INSTALLING
        );
        assert_eq!(
            updater_status_label_id(&S::Skipped { version: ver() }),
            ids::UPDATER_STATUS_SKIPPED
        );
        assert_eq!(
            updater_status_label_id(&S::Error(SmolStr::new_static("e"))),
            ids::UPDATER_STATUS_ERROR
        );
        // Every appended status label id is non-empty in BOTH locales (a blank
        // would paint an empty pill). Spot-check the two endpoints.
        assert!(
            !bentodesk_style::i18n_zh_cn::ZH_CN
                .get(ids::UPDATER_STATUS_IDLE)
                .is_empty()
        );
        assert!(
            !bentodesk_style::i18n_en_us::EN_US
                .get(ids::UPDATER_STATUS_ERROR)
                .is_empty()
        );
    }

    #[test]
    fn visible_version_only_for_versioned_states() {
        assert_eq!(
            updater_visible_version(&S::Available { version: ver() }).map(|v| v.as_str()),
            Some("1.4.0")
        );
        assert!(updater_visible_version(&S::Ready { version: ver() }).is_some());
        assert!(updater_visible_version(&S::Installing { version: ver() }).is_some());
        assert!(updater_visible_version(&S::Skipped { version: ver() }).is_some());
        assert!(updater_visible_version(&S::Idle).is_none());
        assert!(updater_visible_version(&S::Checking).is_none());
        assert!(
            updater_visible_version(&S::Downloading {
                chunk_len: 1,
                total_bytes: Some(2)
            })
            .is_none()
        );
        assert!(updater_visible_version(&S::Error(SmolStr::new_static("e"))).is_none());
    }

    #[test]
    fn progress_fraction_is_chunk_over_total_clamped() {
        // Half-way.
        let f = updater_progress_fraction(&S::Downloading {
            chunk_len: 50,
            total_bytes: Some(100),
        });
        assert!((f.expect("some") - 0.5).abs() < 1e-6);
        // Floor + ceiling.
        assert_eq!(
            updater_progress_fraction(&S::Downloading {
                chunk_len: 0,
                total_bytes: Some(100)
            }),
            Some(0.0)
        );
        // Overshoot clamps to 1.0 (never wider than the track).
        assert_eq!(
            updater_progress_fraction(&S::Downloading {
                chunk_len: 250,
                total_bytes: Some(100)
            }),
            Some(1.0)
        );
    }

    #[test]
    fn progress_fraction_none_when_total_unknown_or_zero() {
        // total_bytes == None → indeterminate (None), never a panic.
        assert_eq!(
            updater_progress_fraction(&S::Downloading {
                chunk_len: 999,
                total_bytes: None
            }),
            None
        );
        // total_bytes == Some(0) → guarded, no divide-by-zero, indeterminate.
        assert_eq!(
            updater_progress_fraction(&S::Downloading {
                chunk_len: 10,
                total_bytes: Some(0)
            }),
            None
        );
        // Non-downloading states paint no bar.
        assert_eq!(updater_progress_fraction(&S::Idle), None);
        assert_eq!(
            updater_progress_fraction(&S::Available { version: ver() }),
            None
        );
    }

    #[test]
    fn height_kind_classifies_every_status_family() {
        use crate::settings_panel::UpdaterHeightKind as K;
        assert_eq!(updater_height_kind(&S::Idle), K::StatusOnly);
        assert_eq!(updater_height_kind(&S::Checking), K::StatusOnly);
        assert_eq!(
            updater_height_kind(&S::UpToDate {
                current_version: ver()
            }),
            K::StatusOnly
        );
        assert_eq!(
            updater_height_kind(&S::Available { version: ver() }),
            K::Versioned
        );
        assert_eq!(
            updater_height_kind(&S::Ready { version: ver() }),
            K::Versioned
        );
        assert_eq!(
            updater_height_kind(&S::Installing { version: ver() }),
            K::Versioned
        );
        assert_eq!(
            updater_height_kind(&S::Skipped { version: ver() }),
            K::Versioned
        );
        assert_eq!(
            updater_height_kind(&S::Downloading {
                chunk_len: 0,
                total_bytes: None
            }),
            K::Downloading
        );
        assert_eq!(
            updater_height_kind(&S::Error(SmolStr::new_static("e"))),
            K::Error
        );
    }

    #[test]
    fn button_visibility_matches_tauri_show_gates() {
        // Download: only Available.
        assert!(updater_show_download(&S::Available { version: ver() }));
        assert!(!updater_show_download(&S::Ready { version: ver() }));
        assert!(!updater_show_download(&S::Idle));
        // Install: only Ready.
        assert!(updater_show_install(&S::Ready { version: ver() }));
        assert!(!updater_show_install(&S::Available { version: ver() }));
        assert!(!updater_show_install(&S::Idle));
        // Skip: Available OR Ready (reuses can_skip_update).
        assert!(updater_show_skip(&S::Available { version: ver() }));
        assert!(updater_show_skip(&S::Ready { version: ver() }));
        assert!(!updater_show_skip(&S::Idle));
        assert!(!updater_show_skip(&S::Checking));
        assert!(!updater_show_skip(&S::Downloading {
            chunk_len: 0,
            total_bytes: None
        }));
    }
}
