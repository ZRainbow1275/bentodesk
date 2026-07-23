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

    // 127..138 M1d (2026-05-29) — orphan slots. Bespoke Advanced + overlay
    // sections deleted; ids 127..=137 blanked in lockstep with zh-CN. Const
    // names removed (no live reference).
    pub const STATE_ENABLED_PILL: StringId = StringId(138);
    pub const STATE_DISABLED_PILL: StringId = StringId(139);
    // 140 Wave I-α / R14 (2026-05-25) — picker row caption matching zh-CN.
    pub const SETTINGS_ZONE_DISPLAY_MODE_LABEL: StringId = StringId(140);

    // 141 M1a (2026-05-29) — Tauri 1:1 parity, General section row 5.
    // Mirrors zh-CN id 141. Lockstep with `SETTING_SPEED_MODE` (117), which
    // is now orphan in both tables.
    pub const SETTING_PORTABLE_MODE: StringId = StringId(141);

    // 142..156 M1d (2026-05-29) — Performance §5 + Startup management §6.
    // Mirrors zh-CN ids 142..=155 slot-for-slot.
    pub const SETTINGS_GROUP_PERFORMANCE: StringId = StringId(142);
    pub const SETTING_EXPAND_DELAY: StringId = StringId(143);
    pub const SETTING_COLLAPSE_DELAY: StringId = StringId(144);
    pub const SETTING_ICON_CACHE_SIZE: StringId = StringId(145);
    pub const SETTINGS_GROUP_STARTUP: StringId = StringId(146);
    pub const SETTING_STARTUP_HIGH_PRIORITY: StringId = StringId(147);
    pub const SETTING_STARTUP_HIGH_PRIORITY_DESC: StringId = StringId(148);
    pub const SETTING_CRASH_RESTART: StringId = StringId(149);
    pub const SETTING_CRASH_RESTART_DESC: StringId = StringId(150);
    pub const SETTING_CRASH_MAX_RETRIES: StringId = StringId(151);
    pub const SETTING_CRASH_WINDOW_SECS: StringId = StringId(152);
    pub const SETTING_SAFE_START_HIBERNATION: StringId = StringId(153);
    pub const SETTING_SAFE_START_HIBERNATION_DESC: StringId = StringId(154);
    pub const SETTING_HIBERNATE_DELAY: StringId = StringId(155);

    // 156..170 M1e (2026-05-29) — Stealth §7 card. Mirrors zh-CN ids
    // 156..=169 slot-for-slot (`src/i18n/locales/en.ts:225-241`).
    pub const STEALTH_GROUP_TITLE: StringId = StringId(156);
    pub const STEALTH_STATUS_LABEL: StringId = StringId(157);
    pub const STEALTH_STATUS_APPLIED: StringId = StringId(158);
    pub const STEALTH_STATUS_PENDING: StringId = StringId(159);
    pub const STEALTH_STATUS_FAILED: StringId = StringId(160);
    pub const STEALTH_SCHEMA_VERSION: StringId = StringId(161);
    pub const STEALTH_MIRROR_HEALTHY: StringId = StringId(162);
    pub const STEALTH_MIRROR_HEALTHY_YES: StringId = StringId(163);
    pub const STEALTH_MIRROR_HEALTHY_NO: StringId = StringId(164);
    pub const STEALTH_RETRY_COUNT: StringId = StringId(165);
    pub const STEALTH_LAST_ERROR: StringId = StringId(166);
    pub const STEALTH_REFRESH_BTN: StringId = StringId(167);
    pub const STEALTH_REAPPLY_BTN: StringId = StringId(168);
    pub const STEALTH_ONEDRIVE_WARNING: StringId = StringId(169);

    // 170..187 M1f (2026-05-29) — Updater §8 card (`UpdaterCard.tsx`).
    // Mirror of the zh-CN ids at the SAME index (positional-array contract;
    // `lookup_tables_have_matching_length` enforces parity). Tauri keys at
    // `src/i18n/locales/en.ts:315-335`.
    pub const UPDATER_CARD_TITLE: StringId = StringId(170);
    pub const UPDATER_STATUS_LABEL: StringId = StringId(171);
    pub const UPDATER_STATUS_IDLE: StringId = StringId(172);
    pub const UPDATER_STATUS_CHECKING: StringId = StringId(173);
    pub const UPDATER_STATUS_UP_TO_DATE: StringId = StringId(174);
    pub const UPDATER_STATUS_AVAILABLE: StringId = StringId(175);
    pub const UPDATER_STATUS_DOWNLOADING: StringId = StringId(176);
    pub const UPDATER_STATUS_READY: StringId = StringId(177);
    pub const UPDATER_STATUS_INSTALLING: StringId = StringId(178);
    pub const UPDATER_STATUS_SKIPPED: StringId = StringId(179);
    pub const UPDATER_STATUS_ERROR: StringId = StringId(180);
    pub const UPDATER_AVAILABLE_VERSION: StringId = StringId(181);
    pub const UPDATER_CHECK_NOW: StringId = StringId(182);
    pub const UPDATER_DOWNLOAD: StringId = StringId(183);
    pub const UPDATER_SKIP_VERSION: StringId = StringId(184);
    pub const UPDATER_INSTALL_RESTART: StringId = StringId(185);
    pub const UPDATER_FREQUENCY: StringId = StringId(186);
    pub const UPDATER_FREQ_DAILY: StringId = StringId(187);
    pub const UPDATER_FREQ_WEEKLY: StringId = StringId(188);
    pub const UPDATER_FREQ_MANUAL: StringId = StringId(189);
    pub const UPDATER_AUTO_DOWNLOAD: StringId = StringId(190);

    // 191..197 M1g (2026-05-29) — Backup §9 card (`BackupCard.tsx`).
    // Mirror of the zh-CN ids at the SAME index (positional-array contract;
    // `lookup_tables_have_matching_length` enforces parity). Tauri keys at
    // `src/i18n/locales/en.ts:338-346`. `BACKUP_REFRESH` has no Tauri key (see
    // the zh-CN note) — it's the nano explicit Refresh affordance.
    pub const BACKUP_CARD_TITLE: StringId = StringId(191);
    pub const BACKUP_CARD_DESCRIPTION: StringId = StringId(192);
    pub const BACKUP_CREATE_NOW: StringId = StringId(193);
    pub const BACKUP_REFRESH: StringId = StringId(194);
    pub const BACKUP_RESTORE: StringId = StringId(195);
    pub const BACKUP_EMPTY: StringId = StringId(196);

    // 197..203 M1h (2026-05-29) — Plugins §11 inline section. Mirror of the
    // zh-CN ids at the SAME index (positional-array contract;
    // `lookup_tables_have_matching_length` enforces parity). Tauri keys at
    // `src/i18n/locales/en.ts:209-218`. Group title reuses `SETTINGS_PLUGINS`
    // (36, "Plugins"); install button is distinct from `BTN_INSTALL` (72) and
    // the empty line is distinct from the long `PLUGINS_EMPTY_HINT` (91).
    pub const PLUGIN_INSTALL: StringId = StringId(197);
    pub const PLUGIN_EMPTY: StringId = StringId(198);
    pub const PLUGIN_TYPE_THEME: StringId = StringId(199);
    pub const PLUGIN_TYPE_WIDGET: StringId = StringId(200);
    pub const PLUGIN_TYPE_ORGANIZER: StringId = StringId(201);
    pub const PLUGIN_UNINSTALL: StringId = StringId(202);
    // 203..205 M1i (2026-05-29) — Paths §2 dynamic desktop-source list
    // (`SettingsPanel.tsx:320-362`, en.ts:30-32). Lockstep with `i18n_zh_cn`
    // (same index/order; `lookup_tables_have_matching_length` enforces parity).
    // User/Public labels reuse `SOURCE_PRIMARY_LABEL` (124) / `SOURCE_PUBLIC_LABEL`
    // (125); these add OneDrive + Custom kind labels and the Watched badge.
    pub const SOURCE_ONEDRIVE_LABEL: StringId = StringId(203);
    pub const SOURCE_CUSTOM_LABEL: StringId = StringId(204);
    pub const SOURCE_WATCHED_BADGE: StringId = StringId(205);
    // 206 M1i fidelity (2026-05-29) — `.desktop-source-empty` placeholder; see
    // the `i18n_zh_cn` note. Mirrors Tauri's `settingsDesktopPathPlaceholder`.
    pub const SOURCE_EMPTY_PLACEHOLDER: StringId = StringId(206);
    // 207..231 M6-UI (2026-05-29) — §3 Appearance inline theme grid; see the
    // `i18n_zh_cn` note. Appended in lockstep (same index, same order). 4
    // family group headings (207-210), 17 theme display names (211-227),
    // Appearance group title (228), theme-picker label (229), accent-colour
    // row label (230). Developer Options ids deferred (not added).
    pub const THEME_GROUP_ROUNDED: StringId = StringId(207);
    pub const THEME_GROUP_SOLID: StringId = StringId(208);
    pub const THEME_GROUP_ANGULAR: StringId = StringId(209);
    pub const THEME_GROUP_PERSONALITY: StringId = StringId(210);
    pub const THEME_NAME_DARK: StringId = StringId(211);
    pub const THEME_NAME_LIGHT: StringId = StringId(212);
    pub const THEME_NAME_MIDNIGHT: StringId = StringId(213);
    pub const THEME_NAME_FOREST: StringId = StringId(214);
    pub const THEME_NAME_SUNSET: StringId = StringId(215);
    pub const THEME_NAME_FROSTED: StringId = StringId(216);
    pub const THEME_NAME_OCEAN_BLUE: StringId = StringId(217);
    pub const THEME_NAME_ROSE_GOLD: StringId = StringId(218);
    pub const THEME_NAME_FOREST_GREEN: StringId = StringId(219);
    pub const THEME_NAME_SOLID: StringId = StringId(220);
    pub const THEME_NAME_ORDER: StringId = StringId(221);
    pub const THEME_NAME_FLAT: StringId = StringId(222);
    pub const THEME_NAME_BRUTALISM: StringId = StringId(223);
    pub const THEME_NAME_EDITORIAL: StringId = StringId(224);
    pub const THEME_NAME_NEO: StringId = StringId(225);
    pub const THEME_NAME_TERMINAL: StringId = StringId(226);
    pub const THEME_NAME_CYBERPUNK: StringId = StringId(227);
    pub const SETTINGS_GROUP_APPEARANCE: StringId = StringId(228);
    pub const THEME_PICKER_LABEL: StringId = StringId(229);
    pub const SETTINGS_ACCENT_COLOR: StringId = StringId(230);

    // 231..244 M7 (2026-06-01) — §10 Encryption card (`EncryptionCard.tsx`).
    // Mirror of the zh-CN ids at the SAME index (positional-array contract;
    // `lookup_tables_have_matching_length` + `zh_cn_en_us_empty_slots_are_in_
    // lockstep` enforce parity). Tauri keys at `src/i18n/locales/en.ts:349-363`.
    pub const ENCRYPTION_CARD_TITLE: StringId = StringId(231);
    pub const ENCRYPTION_CARD_DESC: StringId = StringId(232);
    pub const ENCRYPTION_CURRENT_MODE: StringId = StringId(233);
    pub const ENCRYPTION_MODE_NONE_SUB: StringId = StringId(234);
    pub const ENCRYPTION_MODE_DPAPI_SUB: StringId = StringId(235);
    pub const ENCRYPTION_MODE_PASSPHRASE_FULL: StringId = StringId(236);
    pub const ENCRYPTION_MODE_PASSPHRASE_SUB: StringId = StringId(237);
    pub const ENCRYPTION_PASSPHRASE_LABEL: StringId = StringId(238);
    pub const ENCRYPTION_PASSPHRASE_PLACEHOLDER: StringId = StringId(239);
    pub const ENCRYPTION_PASSPHRASE_HINT: StringId = StringId(240);
    pub const ENCRYPTION_REQUIRED: StringId = StringId(241);
    pub const ENCRYPTION_PROBE_FAILED: StringId = StringId(242);
    pub const ENCRYPTION_MODE_APPLIED: StringId = StringId(243);

    // 244..254 V21-T2/C44 — StackTray / FocusedZonePreview overlay.
    // First four ids mirror Tauri keys; the remaining ids are selected-stack
    // D2D overlay strings that have no separate Tauri TSX component label.
    pub const BULK_MANAGER_COL_ITEMS: StringId = StringId(244);
    pub const STACK_DISSOLVE: StringId = StringId(245);
    pub const STACK_MEMBERS_LABEL: StringId = StringId(246);
    pub const STACK_DETACH_MEMBER: StringId = StringId(247);
    pub const STACK_MORE_MEMBERS: StringId = StringId(248);
    pub const STACK_MORE_STACK_MEMBERS: StringId = StringId(249);
    pub const STACK_REORDER_HINT: StringId = StringId(250);
    pub const FOCUSED_PREVIEW_TITLE: StringId = StringId(251);
    pub const FOCUSED_PREVIEW_EMPTY: StringId = StringId(252);
    pub const STACK_DIMENSION_SEPARATOR: StringId = StringId(253);
    pub const STACK_PREVIEW_ACTIVE: StringId = StringId(254);
    pub const SETTINGS_GROUP_GENERAL: StringId = StringId(255);
    pub const SETTINGS_GROUP_PATHS: StringId = StringId(256);
    // 257..259 Tauri Settings §4 — Zone display mode heading + left copy.
    pub const SETTINGS_GROUP_DISPLAY_MODE: StringId = StringId(257);
    pub const SETTINGS_DISPLAY_MODE_LABEL: StringId = StringId(258);
    pub const SETTINGS_DISPLAY_MODE_HINT: StringId = StringId(259);

    // 260..292 — polished native global Search surface.
    pub const SEARCH_TITLE: StringId = StringId(260);
    pub const SEARCH_CLOSE: StringId = StringId(261);
    pub const SEARCH_PLACEHOLDER: StringId = StringId(262);
    pub const SEARCH_IDLE_HINT: StringId = StringId(263);
    pub const SEARCH_EMPTY: StringId = StringId(264);
    pub const SEARCH_KIND_FILE: StringId = StringId(265);
    pub const SEARCH_KIND_FOLDER: StringId = StringId(266);
    pub const SEARCH_KIND_ZONE: StringId = StringId(267);
    pub const SEARCH_KIND_SETTING: StringId = StringId(268);
    pub const SEARCH_KIND_ACTION: StringId = StringId(269);
    pub const SEARCH_RESULTS_SUFFIX: StringId = StringId(270);
    pub const SEARCH_GROUP_SETTINGS: StringId = StringId(271);
    pub const SEARCH_GROUP_ACTIONS: StringId = StringId(272);
    pub const SEARCH_SETTING_LOCALE: StringId = StringId(273);
    pub const SEARCH_SETTING_UPDATE_FREQUENCY: StringId = StringId(274);
    pub const SEARCH_SETTING_AUTO_DOWNLOAD: StringId = StringId(275);
    pub const SEARCH_SETTING_STEALTH: StringId = StringId(276);
    pub const SEARCH_SETTING_ENCRYPTION: StringId = StringId(277);
    pub const SEARCH_SETTING_ZONE_DISPLAY: StringId = StringId(278);
    pub const SEARCH_SETTING_KEYBINDINGS: StringId = StringId(279);
    pub const SEARCH_SETTING_THEME: StringId = StringId(280);
    pub const SEARCH_ACTION_CREATE_ZONE: StringId = StringId(281);
    pub const SEARCH_ACTION_OPEN_SETTINGS: StringId = StringId(282);
    pub const SEARCH_ACTION_OPEN_ABOUT: StringId = StringId(283);
    pub const SEARCH_ACTION_OPEN_TIMELINE: StringId = StringId(284);
    pub const SEARCH_ACTION_OPEN_SNAPSHOTS: StringId = StringId(285);
    pub const SEARCH_ACTION_OPEN_SUGGESTOR: StringId = StringId(286);
    pub const SEARCH_ACTION_OPEN_BULK_MANAGER: StringId = StringId(287);
    pub const SEARCH_ACTION_OPEN_CAPSULE_PICKER: StringId = StringId(288);
    pub const SEARCH_ACTION_OPEN_RULES: StringId = StringId(289);
    pub const SEARCH_ACTION_LIST_MINIBARS: StringId = StringId(290);
    pub const SEARCH_ACTION_TOGGLE_DEBUG: StringId = StringId(291);
    pub const SEARCH_ACTION_QUIT: StringId = StringId(292);
    pub const PLUGIN_STATUS_INSTALL_CANCELLED: StringId = StringId(293);
    pub const PLUGIN_STATUS_INSTALLED_PREFIX: StringId = StringId(294);
    pub const PLUGIN_STATUS_ENABLED_SUFFIX: StringId = StringId(295);
    pub const PLUGIN_STATUS_DISABLED_SUFFIX: StringId = StringId(296);
    pub const PLUGIN_STATUS_REMOVED_PREFIX: StringId = StringId(297);
    pub const PLUGIN_CONFIRM_UNINSTALL: StringId = StringId(298);
    pub const BTN_CONFIRM: StringId = StringId(299);
    pub const PLUGIN_STATUS_LIST_FAILED_PREFIX: StringId = StringId(300);
    pub const PLUGIN_STATUS_INSTALL_FAILED_PREFIX: StringId = StringId(301);
    pub const PLUGIN_STATUS_TOGGLE_FAILED_PREFIX: StringId = StringId(302);
    pub const PLUGIN_STATUS_UNINSTALL_FAILED_PREFIX: StringId = StringId(303);
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
        "Daily",  // UPDATE_FREQ_DAILY (74)
        "Weekly", // UPDATE_FREQ_WEEKLY (75)
        "Manual", // UPDATE_FREQ_MANUAL (76)
        // 77..80 zone display mode
        "Hover to expand (default)",      // ZONE_MODE_HOVER (77)
        "Always expanded (Fences-style)", // ZONE_MODE_ALWAYS (78)
        "Click to expand",                // ZONE_MODE_CLICK (79)
        // 80..87 encryption
        "Type pass",     // ENCRYPTION_TYPE_PASS (80)
        "Enter unlock",  // ENCRYPTION_ENTER_UNLOCK (81)
        "Enter to set",  // ENCRYPTION_ENTER_TO_SET (82)
        "Unlock",        // ENCRYPTION_UNLOCK (83)
        "None",          // ENCRYPTION_MODE_NONE (84)
        "Windows DPAPI", // ENCRYPTION_MODE_DPAPI (85)
        "Passphrase",    // ENCRYPTION_MODE_PASSPHRASE (86)
        // 87 theme placeholder
        "Default", // THEME_DEFAULT (87)
        // 88..90 updater summary tokens
        "Idle",     // UPDATER_IDLE (88)
        "Checking", // UPDATER_CHECKING (89)
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
        "中文",    // LOCALE_LABEL_ZH_CN (111)
        "English", // LOCALE_LABEL_EN_US (112)
        // 113..121 Round-2 M1 — Settings dark shell top-section + footer.
        "Desktop embed",   // SETTING_DESKTOP_EMBED (113)
        "Run at startup",  // SETTING_AUTOSTART (114)
        "Show in taskbar", // SETTING_SHOW_IN_TASKBAR (115)
        // M1a 2026-05-29 — text retargeted to Tauri "Smart Auto Group"
        // (`en.ts` `settingsAutoGroup:24`).
        "Smart Auto Group", // SETTING_SMART_LAYOUT (116)
        // M1a 2026-05-29 — orphan slot. See zh-CN sibling comment.
        "",         // SETTING_SPEED_MODE (117) — orphan
        "Language", // SETTING_LANGUAGE (118)
        "Cancel",   // SETTING_CANCEL (119)
        "Save",     // SETTING_SAVE (120)
        // 121..127 Round-2 M2 — Desktop sources / path / watch values.
        "Desktop sources",             // SECTION_DESKTOP_SOURCES (121)
        "Desktop path",                // SECTION_DESKTOP_PATH (122)
        "Watch values (one per line)", // SECTION_WATCH_VALUES (123)
        "Personal desktop",            // SOURCE_PRIMARY_LABEL (124)
        "Public desktop",              // SOURCE_PUBLIC_LABEL (125)
        "One per line",                // WATCH_HINT_LINE_EACH (126)
        // 127..138 M1d (2026-05-29) — orphan slots. Bespoke Advanced +
        // overlay sections deleted; blanked in lockstep with zh-CN.
        "",         // (127) — orphan (was SECTION_ADVANCED)
        "",         // (128) — orphan
        "",         // (129) — orphan
        "",         // (130) — orphan
        "",         // (131) — orphan
        "",         // (132) — orphan
        "",         // (133) — orphan
        "",         // (134) — orphan
        "",         // (135) — orphan
        "",         // (136) — orphan
        "",         // (137) — orphan
        "Enabled",  // STATE_ENABLED_PILL (138)
        "Disabled", // STATE_DISABLED_PILL (139)
        // 140 Wave I-α / R14 — picker row caption.
        "Default display mode", // SETTINGS_ZONE_DISPLAY_MODE_LABEL (140)
        // 141 M1a (2026-05-29) — General section row 5 (Tauri parity).
        "Portable Mode (restart required)", // SETTING_PORTABLE_MODE (141)
        // 142..146 M1d — Performance §5 (`SettingsPanel.tsx:601-631`).
        "Performance",     // SETTINGS_GROUP_PERFORMANCE (142)
        "Expand Delay",    // SETTING_EXPAND_DELAY (143)
        "Collapse Delay",  // SETTING_COLLAPSE_DELAY (144)
        "Icon Cache Size", // SETTING_ICON_CACHE_SIZE (145)
        // 146..156 M1d — Startup management §6 (`SettingsPanel.tsx:634-698`).
        "Startup Management",    // SETTINGS_GROUP_STARTUP (146)
        "High Priority Startup", // SETTING_STARTUP_HIGH_PRIORITY (147)
        "Start with high priority for faster response", // SETTING_STARTUP_HIGH_PRIORITY_DESC (148)
        "Crash Auto Restart",    // SETTING_CRASH_RESTART (149)
        "Automatically restart the app after a crash", // SETTING_CRASH_RESTART_DESC (150)
        "Max Retries",           // SETTING_CRASH_MAX_RETRIES (151)
        "Crash Window (s)",      // SETTING_CRASH_WINDOW_SECS (152)
        "Safe Start After Hibernation", // SETTING_SAFE_START_HIBERNATION (153)
        "Start safely after resuming from hibernation", // SETTING_SAFE_START_HIBERNATION_DESC (154)
        "Resume Delay",          // SETTING_HIBERNATE_DELAY (155)
        // 156..170 M1e — Stealth §7 (`StealthModeCard.tsx`, en.ts:225-241).
        "Desktop Stealth Mode", // STEALTH_GROUP_TITLE (156)
        "Status",               // STEALTH_STATUS_LABEL (157)
        "Applied",              // STEALTH_STATUS_APPLIED (158)
        "Pending retry",        // STEALTH_STATUS_PENDING (159)
        "Failed",               // STEALTH_STATUS_FAILED (160)
        "Schema version",       // STEALTH_SCHEMA_VERSION (161)
        "Manifest mirror",      // STEALTH_MIRROR_HEALTHY (162)
        "In sync",              // STEALTH_MIRROR_HEALTHY_YES (163)
        "Out of sync",          // STEALTH_MIRROR_HEALTHY_NO (164)
        "Pending retries",      // STEALTH_RETRY_COUNT (165)
        "Last error",           // STEALTH_LAST_ERROR (166)
        "Refresh status",       // STEALTH_REFRESH_BTN (167)
        "Re-apply",             // STEALTH_REAPPLY_BTN (168)
        "Your Desktop is inside OneDrive. Consider excluding .bentodesk/ from OneDrive sync — otherwise hidden files will be uploaded to the cloud.", // STEALTH_ONEDRIVE_WARNING (169)
        // 170..187 M1f — Updater §8 (`UpdaterCard.tsx`, en.ts:315-335).
        "App Updates",                // UPDATER_CARD_TITLE (170)
        "Status",                     // UPDATER_STATUS_LABEL (171)
        "Up to date",                 // UPDATER_STATUS_IDLE (172)
        "Checking",                   // UPDATER_STATUS_CHECKING (173)
        "Up to date",                 // UPDATER_STATUS_UP_TO_DATE (174)
        "Update available",           // UPDATER_STATUS_AVAILABLE (175)
        "Downloading",                // UPDATER_STATUS_DOWNLOADING (176)
        "Ready to install",           // UPDATER_STATUS_READY (177)
        "Installing",                 // UPDATER_STATUS_INSTALLING (178)
        "Skipped",                    // UPDATER_STATUS_SKIPPED (179)
        "Error",                      // UPDATER_STATUS_ERROR (180)
        "Available",                  // UPDATER_AVAILABLE_VERSION (181)
        "Check now",                  // UPDATER_CHECK_NOW (182)
        "Download",                   // UPDATER_DOWNLOAD (183)
        "Skip this version",          // UPDATER_SKIP_VERSION (184)
        "Install and restart",        // UPDATER_INSTALL_RESTART (185)
        "Check frequency",            // UPDATER_FREQUENCY (186)
        "Daily",                      // UPDATER_FREQ_DAILY (187)
        "Weekly",                     // UPDATER_FREQ_WEEKLY (188)
        "Manual only",                // UPDATER_FREQ_MANUAL (189)
        "Silent background download", // UPDATER_AUTO_DOWNLOAD (190)
        // 191..197 M1g — Backup §9 (`BackupCard.tsx`, en.ts:338-346).
        "Settings Backup", // BACKUP_CARD_TITLE (191)
        "Keeps the last three settings.json snapshots so a bad migration can always be rolled back.", // BACKUP_CARD_DESCRIPTION (192)
        "Backup now",     // BACKUP_CREATE_NOW (193)
        "Refresh",        // BACKUP_REFRESH (194)
        "Restore",        // BACKUP_RESTORE (195)
        "No backups yet", // BACKUP_EMPTY (196)
        // 197..203 M1h — Plugins §11 (`SettingsPanel.tsx:709-781`,
        // en.ts:209-218).
        "Install plugin...",    // PLUGIN_INSTALL (197)
        "No plugins installed", // PLUGIN_EMPTY (198)
        "Theme",                // PLUGIN_TYPE_THEME (199)
        "Widget",               // PLUGIN_TYPE_WIDGET (200)
        "Organizer",            // PLUGIN_TYPE_ORGANIZER (201)
        "Uninstall",            // PLUGIN_UNINSTALL (202)
        // 203..205 M1i — Paths §2 dynamic desktop-source list
        // (`SettingsPanel.tsx:320-362`, en.ts:30-32).
        "OneDrive Desktop", // SOURCE_ONEDRIVE_LABEL (203)
        "Custom Source",    // SOURCE_CUSTOM_LABEL (204)
        "Watched",          // SOURCE_WATCHED_BADGE (205)
        // 206 M1i fidelity — empty `.desktop-source-empty` placeholder.
        "C:\\Users\\...\\Desktop", // SOURCE_EMPTY_PLACEHOLDER (206)
        // 207..231 M6-UI — §3 Appearance inline theme grid
        // (`SettingsPanel.tsx:396-536`, en.ts:163-183).
        "Rounded Glass",  // THEME_GROUP_ROUNDED (207)
        "Solid",          // THEME_GROUP_SOLID (208)
        "Angular Modern", // THEME_GROUP_ANGULAR (209)
        "Personality",    // THEME_GROUP_PERSONALITY (210)
        "Dark",           // THEME_NAME_DARK (211)
        "Light",          // THEME_NAME_LIGHT (212)
        "Midnight",       // THEME_NAME_MIDNIGHT (213)
        "Forest",         // THEME_NAME_FOREST (214)
        "Sunset",         // THEME_NAME_SUNSET (215)
        "Frosted",        // THEME_NAME_FROSTED (216)
        "Ocean Blue",     // THEME_NAME_OCEAN_BLUE (217)
        "Rose Gold",      // THEME_NAME_ROSE_GOLD (218)
        "Forest Green",   // THEME_NAME_FOREST_GREEN (219)
        "Solid",          // THEME_NAME_SOLID (220)
        "Order",          // THEME_NAME_ORDER (221)
        "Flat",           // THEME_NAME_FLAT (222)
        "Brutalism",      // THEME_NAME_BRUTALISM (223)
        "Editorial",      // THEME_NAME_EDITORIAL (224)
        "Neo",            // THEME_NAME_NEO (225)
        "Terminal",       // THEME_NAME_TERMINAL (226)
        "Cyberpunk",      // THEME_NAME_CYBERPUNK (227)
        "Appearance",     // SETTINGS_GROUP_APPEARANCE (228)
        "Choose Theme",   // THEME_PICKER_LABEL (229)
        "Accent Color",   // SETTINGS_ACCENT_COLOR (230)
        // 231..244 M7 — §10 Encryption card (`EncryptionCard.tsx`,
        // en.ts:349-363). Mirror of the zh-CN ids at the SAME index.
        "Settings Encryption", // ENCRYPTION_CARD_TITLE (231)
        "Encryption prevents sensitive settings from leaking when OneDrive / Google Drive sync the AppData folder.", // ENCRYPTION_CARD_DESC (232)
        "Current mode",                   // ENCRYPTION_CURRENT_MODE (233)
        "Default; maximum compatibility", // ENCRYPTION_MODE_NONE_SUB (234)
        "Transparent per-user encryption, no passphrase", // ENCRYPTION_MODE_DPAPI_SUB (235)
        "Passphrase",                     // ENCRYPTION_MODE_PASSPHRASE_FULL (236)
        "AES-256-GCM; survives cross-machine moves", // ENCRYPTION_MODE_PASSPHRASE_SUB (237)
        "Passphrase",                     // ENCRYPTION_PASSPHRASE_LABEL (238)
        "At least 8 characters",          // ENCRYPTION_PASSPHRASE_PLACEHOLDER (239)
        "The passphrase is never stored in plaintext. If you lose it, the encrypted data cannot be recovered.", // ENCRYPTION_PASSPHRASE_HINT (240)
        "Enter a passphrase before switching to passphrase mode", // ENCRYPTION_REQUIRED (241)
        "Passphrase probe failed",                                // ENCRYPTION_PROBE_FAILED (242)
        "Encryption mode switched to",                            // ENCRYPTION_MODE_APPLIED (243)
        // 244..254 V21-T2/C44 — StackTray / FocusedZonePreview overlay.
        "Items",                                     // BULK_MANAGER_COL_ITEMS (244)
        "Dissolve stack",                            // STACK_DISSOLVE (245)
        "Members",                                   // STACK_MEMBERS_LABEL (246)
        "Detach",                                    // STACK_DETACH_MEMBER (247)
        "more members",                              // STACK_MORE_MEMBERS (248)
        "more stack members",                        // STACK_MORE_STACK_MEMBERS (249)
        "Drag over a row to reorder",                // STACK_REORDER_HINT (250)
        "Focused preview",                           // FOCUSED_PREVIEW_TITLE (251)
        "No captured desktop items in this member.", // FOCUSED_PREVIEW_EMPTY (252)
        "·",                                         // STACK_DIMENSION_SEPARATOR (253)
        "Preview open",                              // STACK_PREVIEW_ACTIVE (254)
        "General",                                   // SETTINGS_GROUP_GENERAL (255)
        "Paths",                                     // SETTINGS_GROUP_PATHS (256)
        "Zone Display Mode",                         // SETTINGS_GROUP_DISPLAY_MODE (257)
        "How zones reveal",                          // SETTINGS_DISPLAY_MODE_LABEL (258)
        "Choose how collapsed capsules turn into full panels.", // SETTINGS_DISPLAY_MODE_HINT (259)
        // 260..292 polished native global Search surface.
        "Global Search",                                     // SEARCH_TITLE (260)
        "Close search",                                      // SEARCH_CLOSE (261)
        "Search zones, files, settings, or actions",         // SEARCH_PLACEHOLDER (262)
        "Type a keyword; use arrow keys and Enter to open.", // SEARCH_IDLE_HINT (263)
        "No matching results",                               // SEARCH_EMPTY (264)
        "File",                                              // SEARCH_KIND_FILE (265)
        "Folder",                                            // SEARCH_KIND_FOLDER (266)
        "Zone",                                              // SEARCH_KIND_ZONE (267)
        "Setting",                                           // SEARCH_KIND_SETTING (268)
        "Action",                                            // SEARCH_KIND_ACTION (269)
        " live result(s)",                                   // SEARCH_RESULTS_SUFFIX (270)
        "Settings",                                          // SEARCH_GROUP_SETTINGS (271)
        "Actions",                                           // SEARCH_GROUP_ACTIONS (272)
        "Display language",                                  // SEARCH_SETTING_LOCALE (273)
        "Update check frequency",   // SEARCH_SETTING_UPDATE_FREQUENCY (274)
        "Auto-download updates",    // SEARCH_SETTING_AUTO_DOWNLOAD (275)
        "Desktop stealth storage",  // SEARCH_SETTING_STEALTH (276)
        "Settings encryption mode", // SEARCH_SETTING_ENCRYPTION (277)
        "Zone display mode",        // SEARCH_SETTING_ZONE_DISPLAY (278)
        "Keyboard shortcuts",       // SEARCH_SETTING_KEYBINDINGS (279)
        "Active theme",             // SEARCH_SETTING_THEME (280)
        "Create a new Zone",        // SEARCH_ACTION_CREATE_ZONE (281)
        "Open Settings",            // SEARCH_ACTION_OPEN_SETTINGS (282)
        "Open About",               // SEARCH_ACTION_OPEN_ABOUT (283)
        "Open Timeline",            // SEARCH_ACTION_OPEN_TIMELINE (284)
        "Open layout snapshots",    // SEARCH_ACTION_OPEN_SNAPSHOTS (285)
        "Open smart grouping suggestions", // SEARCH_ACTION_OPEN_SUGGESTOR (286)
        "Open Bulk Manager",        // SEARCH_ACTION_OPEN_BULK_MANAGER (287)
        "Open context capsules",    // SEARCH_ACTION_OPEN_CAPSULE_PICKER (288)
        "Open Rules Wizard",        // SEARCH_ACTION_OPEN_RULES (289)
        "List pinned MiniBars",     // SEARCH_ACTION_LIST_MINIBARS (290)
        "Toggle debug information", // SEARCH_ACTION_TOGGLE_DEBUG (291)
        "Quit BentoDesk",           // SEARCH_ACTION_QUIT (292)
        "Plugin installation cancelled", // PLUGIN_STATUS_INSTALL_CANCELLED (293)
        "Plugin installed: ",       // PLUGIN_STATUS_INSTALLED_PREFIX (294)
        " enabled",                 // PLUGIN_STATUS_ENABLED_SUFFIX (295)
        " disabled",                // PLUGIN_STATUS_DISABLED_SUFFIX (296)
        "Plugin removed: ",         // PLUGIN_STATUS_REMOVED_PREFIX (297)
        "Uninstall this plugin?",   // PLUGIN_CONFIRM_UNINSTALL (298)
        "Confirm",                  // BTN_CONFIRM (299)
        "Plugin list failed: ",     // PLUGIN_STATUS_LIST_FAILED_PREFIX (300)
        "Plugin install failed: ",  // PLUGIN_STATUS_INSTALL_FAILED_PREFIX (301)
        "Plugin toggle failed: ",   // PLUGIN_STATUS_TOGGLE_FAILED_PREFIX (302)
        "Plugin uninstall failed: ", // PLUGIN_STATUS_UNINSTALL_FAILED_PREFIX (303)
    ],
};
