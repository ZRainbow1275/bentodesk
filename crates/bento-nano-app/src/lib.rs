#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-app` — application orchestration.
//!
//! Spec §2: single process; cross-thread comms via `crossbeam-channel`.
//! Spec §9: zero async runtime; the UI thread *is* the GetMessageW loop.
//!
//! This crate wires `bento-nano-tree`, `bento-nano-widget`, `bento-nano-layout`
//! and `bento-nano-platform` together. The render pass lives here so the
//! shell binary stays a thin entry point.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod animator;
pub mod business;
pub mod dispatcher;
pub mod expanded_zone_grid;
pub mod item_file_rename_geometry;
pub mod picker_geometry;
pub mod render;
pub mod settings_panel;
pub mod state;
pub mod theme_bridge;
pub mod theme_picker;
pub mod widgets;
pub mod window_registry;
pub mod zone_editor_geometry;
pub mod zone_pill_geometry;
pub mod zone_surface_geometry;

pub use dispatcher::{
    BulkLayoutAlgorithm, BulkZoneUpdate, Command, CommandReceiver, CommandSender, ContextMenuItem,
    ContextMenuItems, DispatcherError, EventDispatcher, IconHash, IconRequest, ItemId, ItemPath,
    PaletteTarget, Point as DispatchPoint, Request, RequestPair, RequestReceiver, RequestSender,
    SettingValue, Size as DispatchSize, WindowHandle, ZoneSpec, request_channel,
    unhandled_command_log,
};
pub use render::{RenderError, Renderer};
pub use settings_panel::{
    SETTINGS_ACTIVE_THEME_BTN_W, SETTINGS_BACKUP_ENTRY_VISIBLE_MAX, SETTINGS_CLOSE_BTN_H,
    SETTINGS_CLOSE_BTN_W, SETTINGS_PANEL_HEIGHT, SETTINGS_PANEL_PADDING, SETTINGS_PANEL_WIDTH,
    SETTINGS_PLUGINS_ROW_VISIBLE_MAX, SETTINGS_RADIO_GAP, SETTINGS_RADIO_H, SETTINGS_RADIO_INNER_D,
    SETTINGS_RADIO_OUTER_D, SETTINGS_RADIO_W, SETTINGS_SWITCH_BTN_H, SETTINGS_SWITCH_BTN_W,
    SETTINGS_THEME_IMPORT_BTN_W, SETTINGS_UPDATE_ACTION_BTN_W, SETTINGS_UPDATE_ACTION_GAP,
    SETTINGS_UPDATE_SKIP_BTN_W, SETTINGS_ZONE_DISPLAY_MODE_BTN_W, SETTINGS_ZONE_DISPLAY_MODE_COUNT,
    settings_active_theme_rect, settings_backup_entry_rect, settings_backup_list_rect,
    settings_backup_now_rect, settings_backup_restore_rect, settings_close_button_rect,
    settings_encryption_mode_rect, settings_panel_rect, settings_recovery_create_rect,
    settings_recovery_diagnostics_rect, settings_recovery_restore_rect,
    settings_stealth_enabled_rect, settings_switch_button_rect, settings_theme_base_rect,
    settings_theme_import_rect, settings_update_action_rect, settings_update_auto_download_rect,
    settings_update_check_now_rect, settings_update_frequency_rect, settings_update_skip_rect,
    settings_zone_display_mode_picker_row_rect, settings_zone_display_mode_radio_inner_rect,
    settings_zone_display_mode_radio_label_rect, settings_zone_display_mode_radio_outer_rect,
    settings_zone_display_mode_radio_rect, settings_zone_display_mode_rect,
};
pub use state::{
    AppState, IconPickerSession, ItemDragCandidate, ItemFileRenameSession, PalettePickerSession,
    PassphraseEntryPurpose, SettingsBackupEntry, SettingsBackupStatus, SettingsEncryptionMode,
    SettingsKeybindingFeedback, SettingsPluginEntry, SettingsUpdaterStatus, ThemeOption,
    WindowState, ZoneDisplayMode, ZoneEditorSession,
};
pub use window_registry::{MAX_MINIBARS, WindowRegistry, WindowSlot};
