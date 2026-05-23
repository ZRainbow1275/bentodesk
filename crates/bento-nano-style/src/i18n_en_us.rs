//! `en-US` locale. Mirrors `zh_cn` slot-for-slot — see that module's header
//! for numbering rules.

use crate::i18n::{LookupTable, StringId};

pub mod ids {
    use super::StringId;

    // 0..10 application core
    pub const APP_NAME: StringId = StringId(0);
    pub const APP_TAGLINE: StringId = StringId(1);

    // 10..20 toolbar
    pub const TOOLBAR_PIN: StringId = StringId(10);
    pub const TOOLBAR_UNPIN: StringId = StringId(11);
    pub const TOOLBAR_SETTINGS: StringId = StringId(12);
    pub const TOOLBAR_EXIT: StringId = StringId(13);
    pub const TOOLBAR_HIDE: StringId = StringId(14);
    pub const TOOLBAR_SHOW: StringId = StringId(15);

    // 20..40 settings panel labels
    pub const SETTINGS_TITLE: StringId = StringId(20);
    pub const SETTINGS_THEME: StringId = StringId(21);
    pub const SETTINGS_HOTKEY: StringId = StringId(22);
    pub const SETTINGS_LOCALE: StringId = StringId(23);
    pub const SETTINGS_AUTOSTART: StringId = StringId(24);
    pub const SETTINGS_SWITCH: StringId = StringId(25);
    pub const SETTINGS_CLOSE: StringId = StringId(26);
    pub const MENU_SHOW: StringId = StringId(27);
    pub const MENU_EXIT: StringId = StringId(28);

    // G3 wave — settings panel row labels (29..40)
    pub const SETTINGS_UPDATES: StringId = StringId(29);
    pub const SETTINGS_AUTO_DOWNLOAD: StringId = StringId(30);
    pub const SETTINGS_STEALTH_STORAGE: StringId = StringId(31);
    pub const SETTINGS_VAULT_ENCRYPTION: StringId = StringId(32);
    pub const SETTINGS_THEME_HEADING: StringId = StringId(33);
    pub const SETTINGS_VAULT: StringId = StringId(34);
    pub const SETTINGS_KEYS: StringId = StringId(35);
    pub const SETTINGS_PLUGINS: StringId = StringId(36);
    pub const SETTINGS_PERSISTENCE_HINT: StringId = StringId(37);

    // 40..50 errors
    pub const ERROR_DEVICE_LOST: StringId = StringId(40);
    pub const ERROR_FILE_NOT_FOUND: StringId = StringId(41);
    pub const ERROR_PERMISSION: StringId = StringId(42);

    // 50..60 status
    pub const STATUS_LOADING: StringId = StringId(50);
    pub const STATUS_READY: StringId = StringId(51);

    // 60..100 settings panel buttons + enum-driven values (G3 wave)
    pub const BTN_CHECK: StringId = StringId(60);
    pub const BTN_ON: StringId = StringId(61);
    pub const BTN_OFF: StringId = StringId(62);
    pub const BTN_SKIP: StringId = StringId(63);
    pub const BTN_IMPORT: StringId = StringId(64);
    pub const BTN_BACKUP: StringId = StringId(65);
    pub const BTN_LIST: StringId = StringId(66);
    pub const BTN_RESTORE: StringId = StringId(67);
    pub const BTN_BUNDLE: StringId = StringId(68);
    pub const BTN_DIAG: StringId = StringId(69);
    pub const BTN_RECOVER: StringId = StringId(70);
    pub const BTN_DOWNLOAD: StringId = StringId(71);
    pub const BTN_INSTALL: StringId = StringId(72);
    pub const BTN_WAIT: StringId = StringId(73);
    pub const UPDATE_FREQ_DAILY: StringId = StringId(74);
    pub const UPDATE_FREQ_WEEKLY: StringId = StringId(75);
    pub const UPDATE_FREQ_MANUAL: StringId = StringId(76);
    pub const ZONE_MODE_HOVER: StringId = StringId(77);
    pub const ZONE_MODE_ALWAYS: StringId = StringId(78);
    pub const ZONE_MODE_CLICK: StringId = StringId(79);
    pub const ENCRYPTION_TYPE_PASS: StringId = StringId(80);
    pub const ENCRYPTION_ENTER_UNLOCK: StringId = StringId(81);
    pub const ENCRYPTION_ENTER_TO_SET: StringId = StringId(82);
    pub const ENCRYPTION_UNLOCK: StringId = StringId(83);
    pub const ENCRYPTION_MODE_NONE: StringId = StringId(84);
    pub const ENCRYPTION_MODE_DPAPI: StringId = StringId(85);
    pub const ENCRYPTION_MODE_PASSPHRASE: StringId = StringId(86);
    pub const THEME_DEFAULT: StringId = StringId(87);
    pub const UPDATER_IDLE: StringId = StringId(88);
    pub const UPDATER_CHECKING: StringId = StringId(89);
    pub const PLUGINS_REGISTRY_HINT: StringId = StringId(90);
    pub const PLUGINS_EMPTY_HINT: StringId = StringId(91);
    pub const PLUGINS_REFRESH: StringId = StringId(92);
    pub const PLUGINS_REMOVE: StringId = StringId(93);
    pub const KEYBINDINGS_TITLE: StringId = StringId(94);
    pub const KEYBINDINGS_RECORDING: StringId = StringId(95);
    pub const KEYBINDINGS_UNSUPPORTED: StringId = StringId(96);
    pub const KEYBINDINGS_RECORD: StringId = StringId(97);
    pub const KEYBINDINGS_RESET: StringId = StringId(98);

    // 100..111 Wave J1 — theme picker (10 built-in theme presets + picker chrome)
    pub const THEME_DAYLIGHT: StringId = StringId(100);
    pub const THEME_SUNSET: StringId = StringId(101);
    pub const THEME_OCEAN: StringId = StringId(102);
    pub const THEME_FOREST: StringId = StringId(103);
    pub const THEME_LAVENDER: StringId = StringId(104);
    pub const THEME_ROSE: StringId = StringId(105);
    pub const THEME_MIDNIGHT: StringId = StringId(106);
    pub const THEME_MONOCHROME: StringId = StringId(107);
    pub const THEME_EMBER: StringId = StringId(108);
    pub const THEME_PICKER_ACCENT: StringId = StringId(109);
    pub const BTN_SAVE: StringId = StringId(110);

    // Wave K1/K2 — Settings panel locale dropdown labels.
    pub const LOCALE_LABEL_ZH_CN: StringId = StringId(111);
    pub const LOCALE_LABEL_EN_US: StringId = StringId(112);

    // 113..121 Round-2 M1 — Settings dark shell top-section labels + footer.
    pub const SETTING_DESKTOP_EMBED: StringId = StringId(113);
    pub const SETTING_AUTOSTART: StringId = StringId(114);
    pub const SETTING_SHOW_IN_TASKBAR: StringId = StringId(115);
    pub const SETTING_SMART_LAYOUT: StringId = StringId(116);
    pub const SETTING_SPEED_MODE: StringId = StringId(117);
    pub const SETTING_LANGUAGE: StringId = StringId(118);
    pub const SETTING_CANCEL: StringId = StringId(119);
    pub const SETTING_SAVE: StringId = StringId(120);

    // 121..127 Round-2 M2 — Desktop sources / path / watch values.
    pub const SECTION_DESKTOP_SOURCES: StringId = StringId(121);
    pub const SECTION_DESKTOP_PATH: StringId = StringId(122);
    pub const SECTION_WATCH_VALUES: StringId = StringId(123);
    pub const SOURCE_PRIMARY_LABEL: StringId = StringId(124);
    pub const SOURCE_PUBLIC_LABEL: StringId = StringId(125);
    pub const WATCH_HINT_LINE_EACH: StringId = StringId(126);

    // 127..139 Round-2 M3 — Advanced section + overlay version + equipment.
    pub const SECTION_ADVANCED: StringId = StringId(127);
    pub const ROW_ADVANCED_STARTUP: StringId = StringId(128);
    pub const ROW_MAGNET_SWITCH_HINT: StringId = StringId(129);
    pub const ROW_MAX_MAGNET_COUNT: StringId = StringId(130);
    pub const ROW_MAGNET_DURATION: StringId = StringId(131);
    pub const ROW_ZONE_LAYOUT_SECTION: StringId = StringId(132);
    pub const ROW_BAR_COUNT_DISPLAY: StringId = StringId(133);
    pub const SECTION_OVERLAY_VERSION: StringId = StringId(134);
    pub const ROW_OVERLAY_VERSION: StringId = StringId(135);
    pub const ROW_EQUIPMENT_STATE: StringId = StringId(136);
    pub const ROW_MAGNET_STATE: StringId = StringId(137);
    pub const STATE_ENABLED_PILL: StringId = StringId(138);
    pub const STATE_DISABLED_PILL: StringId = StringId(139);
}

pub static EN_US: LookupTable = LookupTable {
    entries: &[
        // 0..2 application core
        "BentoDesk",
        "Desktop Organiser",
        // 2..10 reserved
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        // 10..16 toolbar
        "Pin",
        "Unpin",
        "Settings",
        "Exit",
        "Hide",
        "Show",
        // 16..20 reserved
        "",
        "",
        "",
        "",
        // 20..29 settings panel labels
        "Settings",
        "Theme",
        "Hotkey",
        "Language",
        "Start with system",
        "Switch", // SETTINGS_SWITCH (25)
        "Close",  // SETTINGS_CLOSE (26)
        "Show",   // MENU_SHOW (27)
        "Exit",   // MENU_EXIT (28)
        // 29..38 G3 settings rows
        "Updates",          // SETTINGS_UPDATES (29)
        "Auto-download",    // SETTINGS_AUTO_DOWNLOAD (30)
        "Stealth storage",  // SETTINGS_STEALTH_STORAGE (31)
        "Vault encryption", // SETTINGS_VAULT_ENCRYPTION (32)
        "Theme",            // SETTINGS_THEME_HEADING (33)
        "Vault",            // SETTINGS_VAULT (34)
        "Keys",             // SETTINGS_KEYS (35)
        "Plugins",          // SETTINGS_PLUGINS (36)
        "Config vault persists settings; backup list/restore use real files", // SETTINGS_PERSISTENCE_HINT (37)
        // 38..40 reserved
        "",
        "",
        // 40..43 errors
        "Graphics device disconnected, please restart",
        "File not found",
        "Permission denied",
        // 43..50 reserved
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        // 50..52 status
        "Loading…",
        "Ready",
        // 52..60 reserved
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        // 60..74 generic action buttons
        "Check",    // BTN_CHECK (60)
        "On",       // BTN_ON (61)
        "Off",      // BTN_OFF (62)
        "Skip",     // BTN_SKIP (63)
        "Import",   // BTN_IMPORT (64)
        "Backup",   // BTN_BACKUP (65)
        "List",     // BTN_LIST (66)
        "Restore",  // BTN_RESTORE (67)
        "Bundle",   // BTN_BUNDLE (68)
        "Diag",     // BTN_DIAG (69)
        "Recover",  // BTN_RECOVER (70)
        "Download", // BTN_DOWNLOAD (71)
        "Install",  // BTN_INSTALL (72)
        "Wait",     // BTN_WAIT (73)
        // 74..77 updater frequency
        "Daily",    // UPDATE_FREQ_DAILY (74)
        "Weekly",   // UPDATE_FREQ_WEEKLY (75)
        "Manual",   // UPDATE_FREQ_MANUAL (76)
        // 77..80 zone display mode
        "Mode: Hover",  // ZONE_MODE_HOVER (77)
        "Mode: Always", // ZONE_MODE_ALWAYS (78)
        "Mode: Click",  // ZONE_MODE_CLICK (79)
        // 80..87 encryption
        "Type pass",    // ENCRYPTION_TYPE_PASS (80)
        "Enter unlock", // ENCRYPTION_ENTER_UNLOCK (81)
        "Enter to set", // ENCRYPTION_ENTER_TO_SET (82)
        "Unlock",       // ENCRYPTION_UNLOCK (83)
        "None",         // ENCRYPTION_MODE_NONE (84)
        "Dpapi",        // ENCRYPTION_MODE_DPAPI (85)
        "Passphrase",   // ENCRYPTION_MODE_PASSPHRASE (86)
        // 87 theme placeholder
        "Default",      // THEME_DEFAULT (87)
        // 88..90 updater summary tokens
        "Idle",         // UPDATER_IDLE (88)
        "Checking",     // UPDATER_CHECKING (89)
        // 90..94 plugins modal
        "Registry-backed lifecycle", // PLUGINS_REGISTRY_HINT (90)
        "No installed plugins. Drop pre-extracted plugins into app-data/plugins or use Install for visible archive-gate feedback.", // PLUGINS_EMPTY_HINT (91)
        "Refresh", // PLUGINS_REFRESH (92)
        "Remove",  // PLUGINS_REMOVE (93)
        // 94..99 keybindings modal
        "Shortcuts",    // KEYBINDINGS_TITLE (94)
        "Recording...", // KEYBINDINGS_RECORDING (95)
        "Unsupported",  // KEYBINDINGS_UNSUPPORTED (96)
        "Record",       // KEYBINDINGS_RECORD (97)
        "Reset",        // KEYBINDINGS_RESET (98)
        // 99 reserved
        "",
        // 100..111 Wave J1 — theme picker preset names + picker chrome
        "Daylight",     // THEME_DAYLIGHT (100)
        "Sunset",       // THEME_SUNSET (101)
        "Ocean",        // THEME_OCEAN (102)
        "Forest",       // THEME_FOREST (103)
        "Lavender",     // THEME_LAVENDER (104)
        "Rose",         // THEME_ROSE (105)
        "Midnight",     // THEME_MIDNIGHT (106)
        "Monochrome",   // THEME_MONOCHROME (107)
        "Ember",        // THEME_EMBER (108)
        "Accent color", // THEME_PICKER_ACCENT (109)
        "Save",         // BTN_SAVE (110)
        // 111..113 Wave K1/K2 — locale dropdown labels (native names)
        "中文",         // LOCALE_LABEL_ZH_CN (111)
        "English",      // LOCALE_LABEL_EN_US (112)
        // 113..121 Round-2 M1 — Settings dark shell top-section + footer.
        "Desktop embed",     // SETTING_DESKTOP_EMBED (113)
        "Run at startup",    // SETTING_AUTOSTART (114)
        "Show in taskbar",   // SETTING_SHOW_IN_TASKBAR (115)
        "Smart auto-layout", // SETTING_SMART_LAYOUT (116)
        "Mode (speed mode)", // SETTING_SPEED_MODE (117)
        "Language",          // SETTING_LANGUAGE (118)
        "Cancel",            // SETTING_CANCEL (119)
        "Save",              // SETTING_SAVE (120)
        // 121..127 Round-2 M2 — Desktop sources / path / watch values.
        "Desktop sources",   // SECTION_DESKTOP_SOURCES (121)
        "Desktop path",      // SECTION_DESKTOP_PATH (122)
        "Watch values (one per line)", // SECTION_WATCH_VALUES (123)
        "Personal desktop",  // SOURCE_PRIMARY_LABEL (124)
        "Public desktop",    // SOURCE_PUBLIC_LABEL (125)
        "One per line",      // WATCH_HINT_LINE_EACH (126)
        // 127..139 Round-2 M3 — Advanced + overlay version + equipment.
        "Advanced",                   // SECTION_ADVANCED (127)
        "Advanced startup",           // ROW_ADVANCED_STARTUP (128)
        "Magnet switch hint",         // ROW_MAGNET_SWITCH_HINT (129)
        "Max magnet count",           // ROW_MAX_MAGNET_COUNT (130)
        "Magnet duration (s)",        // ROW_MAGNET_DURATION (131)
        "Quick zone layout",          // ROW_ZONE_LAYOUT_SECTION (132)
        "Salute duration",            // ROW_BAR_COUNT_DISPLAY (133)
        "Future integration check",   // SECTION_OVERLAY_VERSION (134)
        "Architecture version",       // ROW_OVERLAY_VERSION (135)
        "Equipment status",           // ROW_EQUIPMENT_STATE (136)
        "Magnet status",              // ROW_MAGNET_STATE (137)
        "Enabled",                    // STATE_ENABLED_PILL (138)
        "Disabled",                   // STATE_DISABLED_PILL (139)
    ],
};
