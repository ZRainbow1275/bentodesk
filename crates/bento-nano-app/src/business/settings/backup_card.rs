//! BackupCard — settings panel section for rotated-backup management.
//!
//! Visual spec: `backup_card.snap.md`. The selected-stack runtime owns the
//! backing vault commands and renders the compact card directly from
//! `AppState::settings_backup_entries`; this module keeps the widget-tree
//! mount and shared text formatting aligned with that runtime surface.

use bento_nano_style::{Color, Length, i18n_zh_cn::ids as zh_ids};
use bento_nano_tree::{NodeId, TreeError};
use bento_nano_widget::{IconButton, TextNode, WidgetNode};

use crate::business::settings::events as evt;
use crate::state::{AppState, SettingsBackupEntry, SettingsBackupStatus};

use super::default_card_chrome;

/// Build the BackupCard subtree. Returns the chrome Container; the rich
/// composition attaches via [`mount`] which adds children directly into the
/// AppState tree (Container's `child` slot is single, multi-child rows must
/// flow through the Tree's `append_child`).
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_card_chrome())
}

/// "+" icon path — borrowed from `ui::ADD_PATH` shape so the Create Now button
/// reuses the existing 24×24 svg parser subset (M/L only).
const CREATE_NOW_PATH: &str = "M12 5 L12 19 M5 12 L19 12";

/// Mount the BackupCard subtree under `parent`. Returns the card root node id.
///
/// Composition (top-to-bottom, vertical column):
/// - Title text ("Backup") — i18n via `zh_ids::SETTINGS_TITLE` for now
/// - Backup interval row (label + read-only value text)
/// - Backup location row (label + read-only path text)
/// - Create-Now IconButton (event id `evt::BACKUP_CREATE_NOW`)
///
/// Vault-backed values are read at mount time via `Vault::global()`. When the
/// vault is uninitialised the card falls back to documented defaults (§11
/// init-path: log-and-default rather than panic).
pub fn mount(app: &mut AppState, parent: NodeId) -> Result<NodeId, TreeError> {
    let card_id = app.add_child(
        parent,
        "settings_backup_card",
        WidgetNode::Container(default_card_chrome()),
    )?;

    let title = TextNode {
        content: std::borrow::Cow::Borrowed("Backup"),
        id: Some(zh_ids::SETTINGS_TITLE),
        font_size_pt: 13.0,
        font_weight: 500,
        line_height: 1.4,
        color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
        width: Length::Auto,
        height: Length::Px(20.0),
    };
    let _ = app.add_child(card_id, "title", WidgetNode::Text(title));

    let (interval_min, location, retained) = read_backup_settings();

    let interval_label = TextNode {
        content: std::borrow::Cow::Owned(format!("Interval: {interval_min} min")),
        id: None,
        font_size_pt: 11.0,
        font_weight: 400,
        line_height: 1.4,
        color: Color::from_u8(0xA0, 0xA0, 0xB0, 0xFF),
        width: Length::Auto,
        height: Length::Px(16.0),
    };
    let _ = app.add_child(card_id, "interval_row", WidgetNode::Text(interval_label));

    let location_label = TextNode {
        content: std::borrow::Cow::Owned(format!("Location: {location}")),
        id: None,
        font_size_pt: 11.0,
        font_weight: 400,
        line_height: 1.4,
        color: Color::from_u8(0xA0, 0xA0, 0xB0, 0xFF),
        width: Length::Auto,
        height: Length::Px(16.0),
    };
    let _ = app.add_child(card_id, "location_row", WidgetNode::Text(location_label));

    let retained_label = TextNode {
        content: std::borrow::Cow::Owned(format!("Retain: {retained} backups")),
        id: None,
        font_size_pt: 11.0,
        font_weight: 400,
        line_height: 1.4,
        color: Color::from_u8(0xA0, 0xA0, 0xB0, 0xFF),
        width: Length::Auto,
        height: Length::Px(16.0),
    };
    let _ = app.add_child(card_id, "retained_row", WidgetNode::Text(retained_label));

    let backup_status = app.settings_backup_status.borrow().clone();
    if let Some(status) = backup_status.as_ref() {
        let is_error = matches!(status, SettingsBackupStatus::Error(_));
        let status_label = TextNode {
            content: std::borrow::Cow::Owned(backup_status_text(status).to_string()),
            id: None,
            font_size_pt: 11.0,
            font_weight: 500,
            line_height: 1.4,
            color: if is_error {
                Color::from_u8(0xF8, 0x71, 0x71, 0xFF)
            } else {
                Color::from_u8(0x34, 0xD3, 0x99, 0xFF)
            },
            width: Length::Auto,
            height: Length::Px(16.0),
        };
        let _ = app.add_child(card_id, "status_row", WidgetNode::Text(status_label));
    }

    let entries = app.settings_backup_entries.borrow().clone();
    if entries.is_empty() {
        let empty_label = TextNode {
            content: std::borrow::Cow::Borrowed("No backups listed yet"),
            id: None,
            font_size_pt: 11.0,
            font_weight: 400,
            line_height: 1.4,
            color: Color::from_u8(0xA0, 0xA0, 0xB0, 0xFF),
            width: Length::Auto,
            height: Length::Px(16.0),
        };
        let _ = app.add_child(card_id, "backup_empty", WidgetNode::Text(empty_label));
    } else {
        for (entry_index, entry) in entries.iter().take(3).enumerate() {
            let entry_label = TextNode {
                content: std::borrow::Cow::Owned(format_entry_label(entry).to_string()),
                id: None,
                font_size_pt: 11.0,
                font_weight: 400,
                line_height: 1.4,
                color: Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC),
                width: Length::Auto,
                height: Length::Px(16.0),
            };
            let _ = app.add_child(
                card_id,
                format!("backup_entry_{entry_index}"),
                WidgetNode::Text(entry_label),
            );
        }
    }

    let mut btn = IconButton::new(CREATE_NOW_PATH, evt::BACKUP_CREATE_NOW);
    btn.size = 24.0;
    let _ = app.add_child(card_id, "create_now_btn", WidgetNode::IconButton(btn));

    Ok(card_id)
}

/// Read backup-related settings from the global vault. Defaults mirror the 1.x
/// `BackupCard.tsx` baseline (60 min interval, `%APPDATA%/BentoDesk/backups`,
/// 10 retained). Logs + falls back when the vault is unavailable / poisoned.
fn read_backup_settings() -> (i64, smol_str::SmolStr, i64) {
    use bento_nano_backend::config_vault::{SettingValue, Vault};
    let interval_default: i64 = 60;
    let location_default = smol_str::SmolStr::new_static("%APPDATA%/BentoDesk/backups");
    let retained_default: i64 = 10;
    match Vault::global() {
        Some(mtx) => match mtx.lock() {
            Ok(v) => {
                let interval = match v.get_setting("backup.interval_minutes") {
                    Some(SettingValue::Int(n)) => n,
                    _ => interval_default,
                };
                let location = match v.get_setting("backup.location") {
                    Some(SettingValue::Str(s)) => s,
                    _ => location_default.clone(),
                };
                let retained = match v.get_setting("backup.max_retained") {
                    Some(SettingValue::Int(n)) => n,
                    _ => retained_default,
                };
                (interval, location, retained)
            }
            Err(_poisoned) => {
                tracing::warn!(target: "bentodesk::vault", "BackupCard read: vault mutex poisoned; falling back to defaults");
                (interval_default, location_default, retained_default)
            }
        },
        None => {
            tracing::debug!(target: "bentodesk::vault", "BackupCard read: vault not initialised; using defaults");
            (interval_default, location_default, retained_default)
        }
    }
}

/// User-facing one-line label for a listed vault backup. The compact
/// selected-stack Settings overlay has room for three restore buttons, so it
/// shows the stable backup id plus the real file size instead of a mock row.
pub fn format_entry_label(entry: &SettingsBackupEntry) -> smol_str::SmolStr {
    smol_str::SmolStr::new(format!("{} · {}", entry.id, format_size(entry.size_bytes)))
}

/// Extract the visible text from the shared backup/recovery status enum.
pub fn backup_status_text(status: &SettingsBackupStatus) -> &str {
    match status {
        SettingsBackupStatus::Success(text) | SettingsBackupStatus::Error(text) => text.as_str(),
    }
}

/// Format `bytes` using the 1.x convention — `<1024 → "<n> B"`,
/// `<1 MiB → "<x.x> KB"` (one decimal), otherwise `"<x.xx> MB"` (two
/// decimals). Mirrors `updater_card::format_bytes` so the backup-list size
/// column reads identically to the updater download size. Pulled into the
/// port today so the snap.md "size column" text matches the React baseline at
/// composition time.
pub fn format_size(bytes: u64) -> smol_str::SmolStr {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes < KIB {
        // SmolStr::new handles short strings inline (≤ 22 bytes) — the
        // longest "<n> B" output ("1023 B") is 6 bytes, well within inline.
        smol_str::SmolStr::new(format!("{bytes} B"))
    } else if bytes < MIB {
        let kb = (bytes as f64) / KIB as f64;
        smol_str::SmolStr::new(format!("{kb:.1} KB"))
    } else {
        let mb = (bytes as f64) / MIB as f64;
        smol_str::SmolStr::new(format!("{mb:.2} MB"))
    }
}

/// M1g — true when the backup list has no entries (drives the `backupEmpty`
/// placeholder vs the per-row restore list). 1:1 with Tauri
/// `<Show when={backups().length === 0}>` in `BackupCard.tsx`.
pub fn backup_list_is_empty(entries: &[SettingsBackupEntry]) -> bool {
    entries.is_empty()
}

/// M1g — number of backup rows the card actually paints / hit-tests: the live
/// entry count capped at [`crate::settings_panel::SETTINGS_BACKUP_ROW_VISIBLE_MAX`].
/// Both the renderer's `for` loop and the shell hit-tester feed this into the
/// row geometry so paint and hit agree, and the dynamic body height matches
/// whatever rows are on screen.
pub fn backup_visible_row_count(entries: &[SettingsBackupEntry]) -> usize {
    entries
        .len()
        .min(crate::settings_panel::SETTINGS_BACKUP_ROW_VISIBLE_MAX)
}

/// M1g — map a visible-row index back to its backup id, mirroring the shell
/// dispatch arm (`SettingsHit::RestoreSettingsBackup(index)` →
/// `entries.get(index).id`). Returns `None` for an out-of-range index so a
/// stale click after the list shrank can never panic. Borrows the
/// already-allocated id so the lookup stays alloc-free.
pub fn backup_entry_id_at(
    entries: &[SettingsBackupEntry],
    index: usize,
) -> Option<&smol_str::SmolStr> {
    entries.get(index).map(|entry| &entry.id)
}

/// Convert a 1.x backup ISO basic-format timestamp ("20260418T150000.000Z")
/// to the human-readable form ("2026-04-18 15:00:00 UTC"). Returns `raw`
/// verbatim when the input doesn't match the expected 8-digit-date + T +
/// 6-digit-time prefix — defensive parse so a malformed manifest never
/// panics the UI.
pub fn format_timestamp(raw: &str) -> smol_str::SmolStr {
    // Pattern: YYYYMMDDTHHMMSS — exactly 15 chars before the optional
    // ".XXX[Z]" suffix. Bail out on length / non-digit early so the slice
    // indexing below is safe without panic.
    if raw.len() < 15 || raw.as_bytes()[8] != b'T' {
        return smol_str::SmolStr::new(raw);
    }
    let bytes = raw.as_bytes();
    if !bytes[..8]
        .iter()
        .chain(&bytes[9..15])
        .all(u8::is_ascii_digit)
    {
        return smol_str::SmolStr::new(raw);
    }
    let y = &raw[0..4];
    let mo = &raw[4..6];
    let d = &raw[6..8];
    let h = &raw[9..11];
    let mi = &raw[11..13];
    let s = &raw[13..15];
    smol_str::SmolStr::new(format!("{y}-{mo}-{d} {h}:{mi}:{s} UTC"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_below_1k_emits_bytes_unit() {
        assert_eq!(format_size(0).as_str(), "0 B");
        assert_eq!(format_size(1023).as_str(), "1023 B");
    }

    #[test]
    fn format_size_kib_range_emits_one_decimal_kb() {
        // 1024 bytes = exactly the KB floor; 1536 = 1.5 KB; just under 1 MiB
        // still reads in KB (pinning the 1.x decimal precision + boundary).
        assert_eq!(format_size(1024).as_str(), "1.0 KB");
        assert_eq!(format_size(1536).as_str(), "1.5 KB");
        assert_eq!(format_size(1024 * 1024 - 1).as_str(), "1024.0 KB");
    }

    #[test]
    fn format_size_mib_range_emits_two_decimal_mb() {
        // 1 MiB is the MB floor; 2.5 MiB pins the two-decimal precision.
        assert_eq!(format_size(1024 * 1024).as_str(), "1.00 MB");
        assert_eq!(format_size(2 * 1024 * 1024 + 512 * 1024).as_str(), "2.50 MB");
    }

    #[test]
    fn backup_list_empty_predicate_tracks_entry_count() {
        assert!(backup_list_is_empty(&[]));
        let one = vec![SettingsBackupEntry {
            id: smol_str::SmolStr::new_static("a"),
            file_name: smol_str::SmolStr::new_static("vault-a.bin"),
            size_bytes: 10,
        }];
        assert!(!backup_list_is_empty(&one));
    }

    #[test]
    fn backup_visible_row_count_caps_at_max() {
        use crate::settings_panel::SETTINGS_BACKUP_ROW_VISIBLE_MAX;
        let mk = |n: usize| -> Vec<SettingsBackupEntry> {
            (0..n)
                .map(|i| SettingsBackupEntry {
                    id: smol_str::SmolStr::new(format!("id-{i}")),
                    file_name: smol_str::SmolStr::new(format!("vault-{i}.bin")),
                    size_bytes: i as u64,
                })
                .collect()
        };
        // 0 / few (under cap) / exactly cap / over cap.
        assert_eq!(backup_visible_row_count(&mk(0)), 0);
        assert_eq!(backup_visible_row_count(&mk(2)), 2);
        assert_eq!(
            backup_visible_row_count(&mk(SETTINGS_BACKUP_ROW_VISIBLE_MAX)),
            SETTINGS_BACKUP_ROW_VISIBLE_MAX
        );
        assert_eq!(
            backup_visible_row_count(&mk(SETTINGS_BACKUP_ROW_VISIBLE_MAX + 5)),
            SETTINGS_BACKUP_ROW_VISIBLE_MAX
        );
    }

    #[test]
    fn backup_entry_id_at_maps_index_to_id_and_guards_out_of_range() {
        let entries = vec![
            SettingsBackupEntry {
                id: smol_str::SmolStr::new_static("newest"),
                file_name: smol_str::SmolStr::new_static("vault-newest.bin"),
                size_bytes: 1,
            },
            SettingsBackupEntry {
                id: smol_str::SmolStr::new_static("older"),
                file_name: smol_str::SmolStr::new_static("vault-older.bin"),
                size_bytes: 2,
            },
        ];
        assert_eq!(
            backup_entry_id_at(&entries, 0).map(|s| s.as_str()),
            Some("newest")
        );
        assert_eq!(
            backup_entry_id_at(&entries, 1).map(|s| s.as_str()),
            Some("older")
        );
        // Stale click past the end → None, never a panic.
        assert_eq!(backup_entry_id_at(&entries, 2), None);
    }

    #[test]
    fn format_timestamp_well_formed_returns_human_readable() {
        let raw = "20260418T150000.000Z";
        assert_eq!(format_timestamp(raw).as_str(), "2026-04-18 15:00:00 UTC");
    }

    #[test]
    fn format_timestamp_malformed_returns_raw() {
        // Shorter than 15 chars — bail out without panic.
        assert_eq!(format_timestamp("nope").as_str(), "nope");
        // 16 chars but missing the 'T' separator — bail out.
        assert_eq!(
            format_timestamp("2026041815000000").as_str(),
            "2026041815000000"
        );
    }

    #[test]
    fn format_entry_label_uses_real_id_and_size() {
        let entry = SettingsBackupEntry {
            id: smol_str::SmolStr::new_static("200-new"),
            file_name: smol_str::SmolStr::new_static("vault-200-new.bin"),
            size_bytes: 1536,
        };

        assert_eq!(format_entry_label(&entry).as_str(), "200-new · 1.5 KB");
    }
}
