//! Business surface — Settings family (Panel + 5 cards).
//!
//! Visual specs:
//! - `settings_panel.snap.md` (host modal, 480 px wide × max-height 80vh)
//! - `backup_card.snap.md` (rotated settings.json restore + create-now)
//! - `encryption_card.snap.md` (None / DPAPI / Passphrase mode picker)
//! - `keybindings_section.snap.md` (chord recorder + reset)
//! - `stealth_mode_card.snap.md` (R3 stealth status + reapply + OneDrive warning)
//! - `updater_card.snap.md` (check / download / install + frequency dropdown)
//!
//! Runtime status: selected-stack complete. The visible Settings surface is
//! rendered by `Renderer::draw_settings_panel` from live `AppState` values:
//! updater lifecycle, plugin/theme management, vault backup/recovery,
//! encryption, keybindings, locale, stealth storage, and display-mode actions
//! are all routed through shell commands. This module remains the lightweight
//! widget-tree compatibility layer used by `Command::OpenSettings` to mount
//! stable ids and per-card event contracts; it is not the user-visible drawing
//! implementation.
//!
//! Public API: every card + the panel exposes a `build()` and `mount()` path
//! so existing widget-tree callers keep stable entry points. The shell mounts
//! the panel via `WindowKind::Settings` HWND and the D2D renderer paints the
//! complete runtime surface directly from `AppState`.

use bentodesk_layout::Direction;
use bentodesk_style::{Edges, Length};
use bentodesk_widget::{ContainerNode, WidgetNode};

pub mod backup_card;
pub mod encryption_card;
pub mod keybindings_section;
pub mod panel;
// M1h (2026-05-29) — Plugins §11 inline section view-model helpers. Unlike the
// K1 cards this has no widget-tree `build`/`mount` (the plugins surface was
// never tree-mounted; it is hand-painted by `Renderer::draw_settings_panel`).
pub mod plugins_section;
pub mod stealth_mode_card;
pub mod updater_card;

/// Re-export each card's `build()` at the module root for ergonomic mount
/// from the SettingsPanel host body. Naming matches the 1.x file basename.
pub use backup_card::build as build_backup_card;
pub use encryption_card::build as build_encryption_card;
pub use keybindings_section::build as build_keybindings_section;
pub use panel::build as build_settings_panel;
pub use stealth_mode_card::build as build_stealth_mode_card;
pub use updater_card::build as build_updater_card;

/// Default chrome a Settings card uses while the real composition lands.
/// Centralises the `settings-card`-equivalent geometry (12 px corner radius,
/// 16 px padding, vertical column) so each card's `build()` body picks the
/// same chrome in one call site.
pub(crate) fn default_card_chrome() -> ContainerNode {
    ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::all(16.0),
        ..ContainerNode::default()
    }
}

/// Default chrome for the Settings host panel — a single column body with
/// the snap.md mandated geometry tokens (16 px corner radius surface,
/// 24 px padding inside the modal). Exposed `pub(crate)` so the panel
/// module's `build()` reuses the same Container shape today.
pub(crate) fn default_panel_chrome() -> ContainerNode {
    ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::all(24.0),
        ..ContainerNode::default()
    }
}

/// Public free function — builds the topmost Settings UI subtree used by
/// legacy tree callers. The selected-stack runtime surface is painted by the
/// D2D renderer; [`mount`] below attaches the stable card ids and event hooks.
pub fn build() -> WidgetNode {
    WidgetNode::Container(default_panel_chrome())
}

/// Per-card widget event ids. Each card's interactive primitives carry one of
/// these as their `on_change_event` so the shell's wndproc click router can
/// translate id → `Command::SetSetting { key, value }`.
///
/// Layout: a single contiguous u32 range starting at `SETTINGS_EVENT_BASE`
/// (10_000) so collisions with toolbar event ids 1..=5 (see
/// `bentodesk-shell::ui::events`) are impossible.
///
/// Wire format / shell routing: per `prompts/0503/04-security-backup.md` the
/// click router (F3 wave) translates each id to a `(key, SettingValue)` tuple.
/// The `key` strings are the snap.md mandated dotted-path tokens; the value
/// type is documented in the per-id doc-comment below.
pub mod events {
    /// Toolbar event ids end at 5; 10_000 leaves ample headroom + makes the
    /// range visually obvious in trace logs.
    pub const SETTINGS_EVENT_BASE: u32 = 10_000;

    // ---- backup_card ----
    /// "Create Now" button click. No payload — F3 wires
    /// `Command::BackupNow`.
    pub const BACKUP_CREATE_NOW: u32 = SETTINGS_EVENT_BASE + 1;

    // ---- encryption_card ----
    /// Mode dropdown change. Value: `SettingValue::Str(EncryptionMode::as_wire)`.
    /// Key: `"encryption.mode"`.
    pub const ENCRYPTION_MODE_CHANGE: u32 = SETTINGS_EVENT_BASE + 10;
    /// Passphrase Input commit. Value: `SettingValue::Str(passphrase)`.
    /// Key: `"encryption.passphrase"`.
    pub const ENCRYPTION_PASSPHRASE_COMMIT: u32 = SETTINGS_EVENT_BASE + 11;

    // ---- keybindings_section ----
    /// Read-only row click — F4 wave will turn this into a chord recorder
    /// trigger. Today the click is logged + dropped.
    pub const KEYBINDINGS_ROW_CLICK: u32 = SETTINGS_EVENT_BASE + 20;

    // ---- stealth_mode_card ----
    /// Stealth enable toggle. Value: `SettingValue::Bool`. Key: `"stealth.enabled"`.
    pub const STEALTH_TOGGLE_ENABLED: u32 = SETTINGS_EVENT_BASE + 30;
    /// Hidden-by-default toggle. Value: `SettingValue::Bool`. Key:
    /// `"stealth.hidden_by_default"`.
    pub const STEALTH_TOGGLE_HIDDEN: u32 = SETTINGS_EVENT_BASE + 31;
    /// Exclude-from-screen-capture toggle (wired to WDA_EXCLUDEFROMCAPTURE in
    /// F6). Value: `SettingValue::Bool`. Key: `"stealth.exclude_from_capture"`.
    pub const STEALTH_TOGGLE_EXCLUDE_CAPTURE: u32 = SETTINGS_EVENT_BASE + 32;

    // ---- updater_card ----
    /// Update-check cadence dropdown. Value:
    /// `SettingValue::Str("Daily" | "Weekly" | "Manual")`.
    /// Key: `"updates.check_frequency"`.
    pub const UPDATER_FREQUENCY_CHANGE: u32 = SETTINGS_EVENT_BASE + 40;
    /// Auto-download toggle. Value: `SettingValue::Bool`.
    /// Key: `"updates.auto_download"`.
    pub const UPDATER_TOGGLE_AUTO_DOWNLOAD: u32 = SETTINGS_EVENT_BASE + 41;
    /// "Check Now" button click. No payload — F3 wires
    /// `Command::UpdaterCheckNow`.
    pub const UPDATER_CHECK_NOW: u32 = SETTINGS_EVENT_BASE + 42;
}

/// Mount the Settings panel subtree (chrome + all 5 cards) as a child of
/// `parent` and return the panel's root node id. Caller (shell
/// `Command::OpenSettings`) caches the returned id and skips re-mounting on
/// subsequent opens.
///
/// Builds: Panel container → [BackupCard, EncryptionCard, KeybindingsSection,
/// StealthModeCard, UpdaterCard]. Each card's vault-backed values are read at
/// mount time; primitives' `on_change_event` ids come from the [`events`]
/// module so the shell click router can translate id → `Command::SetSetting`.
pub fn mount(
    app: &mut crate::state::AppState,
    parent: bentodesk_tree::NodeId,
) -> Result<bentodesk_tree::NodeId, bentodesk_tree::TreeError> {
    let panel_id = app.add_child(
        parent,
        "settings_panel",
        WidgetNode::Container(default_panel_chrome()),
    )?;
    let _ = backup_card::mount(app, panel_id);
    let _ = encryption_card::mount(app, panel_id);
    let _ = keybindings_section::mount(app, panel_id);
    let _ = stealth_mode_card::mount(app, panel_id);
    let _ = updater_card::mount(app, panel_id);
    Ok(panel_id)
}
