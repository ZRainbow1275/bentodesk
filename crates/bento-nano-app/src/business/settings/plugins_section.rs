//! PluginsSection — inline Settings §11 plugin-management section.
//!
//! Visual spec: Tauri `SettingsPanel.tsx:709-781` (`settings-group` →
//! `安装插件...` button → `plugin-list` of `plugin-card`s). M1h (2026-05-29)
//! moved nano's plugins surface from a gated in-panel modal to this always-
//! inline section of the dark scrollable Settings body.
//!
//! The selected-stack runtime owns the backing registry commands (install via
//! a native file picker → `plugins::loader`; toggle/uninstall → registry
//! mutations) and renders the compact cards directly from
//! `AppState::settings_plugin_entries`. This module keeps the PURE, testable
//! view-model helpers the renderer + shell hit-tester share: type-badge label
//! selection, the visible-row cap, the empty-state predicate, and the
//! row-index → entry mapping that the Toggle/Uninstall dispatch arms rely on.
//!
//! No widget-tree `build`/`mount` here (unlike the K1 cards): the plugins
//! surface was never tree-mounted — it is hand-painted by
//! `Renderer::draw_settings_panel` from these helpers.

use bento_nano_style::{StringId, i18n_zh_cn::ids as zh_ids};

use crate::settings_panel::SETTINGS_PLUGINS_ROW_VISIBLE_MAX;
use crate::state::SettingsPluginEntry;

/// M1h — i18n id for a plugin's type badge, mirroring Tauri
/// `pluginTypeLabelKey` (`SettingsPanel.tsx:187-194`). The entry's
/// `plugin_type` is the lowercase wire token the backend writes
/// (`plugin_type_label`: "theme" / "widget" / "organizer"). Any unrecognised
/// token falls back to the Theme badge so the card never renders a blank badge
/// (defensive — a malformed registry row can't blank the UI).
pub fn plugin_type_label_id(plugin_type: &str) -> StringId {
    match plugin_type {
        "widget" => zh_ids::PLUGIN_TYPE_WIDGET,
        "organizer" => zh_ids::PLUGIN_TYPE_ORGANIZER,
        // "theme" + anything unexpected.
        _ => zh_ids::PLUGIN_TYPE_THEME,
    }
}

/// M1h — true when the plugin list has no entries (drives the `pluginEmpty`
/// placeholder vs the per-card list). 1:1 with Tauri
/// `<Show when={!pluginsLoading() && plugins().length === 0}>`
/// (`SettingsPanel.tsx:723`).
pub fn plugin_list_is_empty(entries: &[SettingsPluginEntry]) -> bool {
    entries.is_empty()
}

/// M1h — number of plugin cards the section actually paints / hit-tests: the
/// live entry count capped at [`SETTINGS_PLUGINS_ROW_VISIBLE_MAX`]. Both the
/// renderer's `for` loop and the shell hit-tester feed this into the card
/// geometry so paint and hit agree, and the dynamic body height matches
/// whatever cards are on screen.
pub fn plugin_visible_row_count(entries: &[SettingsPluginEntry]) -> usize {
    entries.len().min(SETTINGS_PLUGINS_ROW_VISIBLE_MAX)
}

/// M1h — map a visible-card index back to its plugin id, mirroring the shell
/// dispatch arms (`SettingsHit::TogglePlugin(idx)` / `UninstallPlugin(idx)` →
/// `entries.get(idx).id`). Returns `None` for an out-of-range index so a stale
/// click after the list shrank can never panic. Borrows the already-allocated
/// id so the lookup stays alloc-free.
pub fn plugin_entry_id_at(
    entries: &[SettingsPluginEntry],
    index: usize,
) -> Option<&smol_str::SmolStr> {
    entries.get(index).map(|entry| &entry.id)
}

/// M1h — the (id, next-enabled) pair a `TogglePlugin(index)` click resolves to,
/// mirroring the shell dispatch arm (`entries.get(idx) → (id, !enabled)`).
/// Returns `None` for an out-of-range index. Keeps the "flip the current
/// enabled state" rule in the lib crate so the bin dispatch stays thin and the
/// rule is unit-tested.
pub fn plugin_toggle_target(
    entries: &[SettingsPluginEntry],
    index: usize,
) -> Option<(smol_str::SmolStr, bool)> {
    entries
        .get(index)
        .map(|entry| (entry.id.clone(), !entry.enabled))
}

/// M1h — one-line "name · v{version}" header label for a plugin card. Mirrors
/// the Tauri header which renders `{name}` + `v{version}` side by side
/// (`SettingsPanel.tsx:733-734`). The type badge + toggle paint separately to
/// the right, so this is just the leading name/version run.
pub fn format_plugin_header(entry: &SettingsPluginEntry) -> smol_str::SmolStr {
    smol_str::SmolStr::new(format!("{} · v{}", entry.name, entry.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        id: &str,
        name: &str,
        version: &str,
        plugin_type: &str,
        enabled: bool,
    ) -> SettingsPluginEntry {
        SettingsPluginEntry {
            id: smol_str::SmolStr::new(id),
            name: smol_str::SmolStr::new(name),
            version: smol_str::SmolStr::new(version),
            plugin_type: smol_str::SmolStr::new(plugin_type),
            author: smol_str::SmolStr::new("Acme"),
            description: smol_str::SmolStr::new("A plugin"),
            enabled,
        }
    }

    fn mk(n: usize) -> Vec<SettingsPluginEntry> {
        (0..n)
            .map(|i| {
                entry(
                    &format!("id-{i}"),
                    &format!("Plugin {i}"),
                    "1.0.0",
                    "theme",
                    i % 2 == 0,
                )
            })
            .collect()
    }

    #[test]
    fn type_badge_label_selection_per_kind() {
        // Each recognised wire token resolves to its own badge id.
        assert_eq!(plugin_type_label_id("theme"), zh_ids::PLUGIN_TYPE_THEME);
        assert_eq!(plugin_type_label_id("widget"), zh_ids::PLUGIN_TYPE_WIDGET);
        assert_eq!(
            plugin_type_label_id("organizer"),
            zh_ids::PLUGIN_TYPE_ORGANIZER
        );
        // Unknown / malformed token falls back to Theme (never blank).
        assert_eq!(plugin_type_label_id(""), zh_ids::PLUGIN_TYPE_THEME);
        assert_eq!(plugin_type_label_id("bogus"), zh_ids::PLUGIN_TYPE_THEME);
        // The three badge ids are distinct so the badges never collide.
        assert_ne!(zh_ids::PLUGIN_TYPE_THEME, zh_ids::PLUGIN_TYPE_WIDGET);
        assert_ne!(zh_ids::PLUGIN_TYPE_WIDGET, zh_ids::PLUGIN_TYPE_ORGANIZER);
        assert_ne!(zh_ids::PLUGIN_TYPE_THEME, zh_ids::PLUGIN_TYPE_ORGANIZER);
    }

    #[test]
    fn empty_state_predicate_tracks_entry_count() {
        assert!(plugin_list_is_empty(&[]));
        assert!(!plugin_list_is_empty(&mk(1)));
    }

    #[test]
    fn visible_row_count_caps_at_max() {
        // 0 / few (under cap) / exactly cap / over cap.
        assert_eq!(plugin_visible_row_count(&mk(0)), 0);
        assert_eq!(plugin_visible_row_count(&mk(2)), 2);
        assert_eq!(
            plugin_visible_row_count(&mk(SETTINGS_PLUGINS_ROW_VISIBLE_MAX)),
            SETTINGS_PLUGINS_ROW_VISIBLE_MAX
        );
        assert_eq!(
            plugin_visible_row_count(&mk(SETTINGS_PLUGINS_ROW_VISIBLE_MAX + 5)),
            SETTINGS_PLUGINS_ROW_VISIBLE_MAX
        );
    }

    #[test]
    fn entry_id_at_maps_index_and_guards_out_of_range() {
        let entries = vec![
            entry("first", "First", "1.0.0", "theme", true),
            entry("second", "Second", "2.0.0", "widget", false),
        ];
        assert_eq!(
            plugin_entry_id_at(&entries, 0).map(|s| s.as_str()),
            Some("first")
        );
        assert_eq!(
            plugin_entry_id_at(&entries, 1).map(|s| s.as_str()),
            Some("second")
        );
        // Stale click past the end → None, never a panic.
        assert_eq!(plugin_entry_id_at(&entries, 2), None);
    }

    #[test]
    fn toggle_target_flips_enabled_and_maps_index() {
        let entries = vec![
            entry("on", "On", "1.0.0", "theme", true),
            entry("off", "Off", "1.0.0", "widget", false),
        ];
        // Enabled row → toggles to disabled (false).
        assert_eq!(
            plugin_toggle_target(&entries, 0).map(|(id, en)| (id.to_string(), en)),
            Some(("on".to_string(), false))
        );
        // Disabled row → toggles to enabled (true).
        assert_eq!(
            plugin_toggle_target(&entries, 1).map(|(id, en)| (id.to_string(), en)),
            Some(("off".to_string(), true))
        );
        // Out-of-range → None.
        assert_eq!(plugin_toggle_target(&entries, 9), None);
    }

    #[test]
    fn header_label_uses_name_and_version() {
        let e = entry("id", "Acme Theme", "2.3.1", "theme", true);
        assert_eq!(format_plugin_header(&e).as_str(), "Acme Theme · v2.3.1");
    }
}
