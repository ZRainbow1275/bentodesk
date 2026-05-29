//! `zh-CN` locale — the BentoDesk baseline.
//!
//! ## Numbering rules (read before adding strings)
//!
//! - **Never re-number existing ids.** Tables in different locales index by
//!   the same `StringId.0`; renumbering one without the other silently
//!   translates the wrong string in production.
//! - **Group by feature, leave reserved gaps.** New strings inside an
//!   existing feature go in the reserved slots (`""` placeholder); the
//!   feature's id range stays contiguous.
//! - **Group ranges**:
//!   - `0..10` — Application core
//!   - `10..20` — Toolbar
//!   - `20..40` — Settings panel labels (rows + title-bar buttons)
//!   - `40..50` — Errors (user-visible)
//!   - `50..60` — Status / loading
//!   - `60..100` — Settings panel buttons + enum-driven values (G3 wave)
//!
//! Both `zh_cn` and `en_us` honour this layout. If you add a new range,
//! pad it identically in every locale to keep `LookupTable.len()` in sync.

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
    /// Locale-switch button label inside the settings panel (Phase 2.1 / Ruling C).
    pub const SETTINGS_SWITCH: StringId = StringId(25);
    /// Settings panel close button (Phase 2.1 / Ruling C).
    pub const SETTINGS_CLOSE: StringId = StringId(26);
    /// Tray popup-menu「显示」entry (Phase 2.1 / Ruling B). Distinct from
    /// `TOOLBAR_SHOW` so the locale strings can diverge later (e.g. tray
    /// could say "Restore" while toolbar says "Show").
    pub const MENU_SHOW: StringId = StringId(27);
    /// Tray popup-menu「退出」entry (Phase 2.1 / Ruling B).
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
    // 38..40 reserved for future row labels

    // 40..50 errors (user-visible)
    pub const ERROR_DEVICE_LOST: StringId = StringId(40);
    pub const ERROR_FILE_NOT_FOUND: StringId = StringId(41);
    pub const ERROR_PERMISSION: StringId = StringId(42);

    // 50..60 status
    pub const STATUS_LOADING: StringId = StringId(50);
    pub const STATUS_READY: StringId = StringId(51);

    // 60..100 settings panel buttons + enum-driven values (G3 wave)
    // 60..74 generic action buttons
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
    // 74..77 updater frequency
    pub const UPDATE_FREQ_DAILY: StringId = StringId(74);
    pub const UPDATE_FREQ_WEEKLY: StringId = StringId(75);
    pub const UPDATE_FREQ_MANUAL: StringId = StringId(76);
    // 77..80 zone display mode — short radio-button labels (Wave I-α / R14).
    // Previously held "模式: 悬停" prefixed strings used by the cycle button;
    // R14 reshape (2026-05-25): prefix moved to id 140 so radios can compose
    // `默认显示模式  ○ 悬停  ○ 常显  ● 点击` without doubling up "模式:".
    pub const ZONE_MODE_HOVER: StringId = StringId(77);
    pub const ZONE_MODE_ALWAYS: StringId = StringId(78);
    pub const ZONE_MODE_CLICK: StringId = StringId(79);
    // 80..87 encryption row states / mode names
    pub const ENCRYPTION_TYPE_PASS: StringId = StringId(80);
    pub const ENCRYPTION_ENTER_UNLOCK: StringId = StringId(81);
    pub const ENCRYPTION_ENTER_TO_SET: StringId = StringId(82);
    pub const ENCRYPTION_UNLOCK: StringId = StringId(83);
    pub const ENCRYPTION_MODE_NONE: StringId = StringId(84);
    pub const ENCRYPTION_MODE_DPAPI: StringId = StringId(85);
    pub const ENCRYPTION_MODE_PASSPHRASE: StringId = StringId(86);
    // 87 theme placeholder
    pub const THEME_DEFAULT: StringId = StringId(87);
    // 88..90 updater status summary tokens
    pub const UPDATER_IDLE: StringId = StringId(88);
    pub const UPDATER_CHECKING: StringId = StringId(89);
    // 90..94 plugins modal
    pub const PLUGINS_REGISTRY_HINT: StringId = StringId(90);
    pub const PLUGINS_EMPTY_HINT: StringId = StringId(91);
    pub const PLUGINS_REFRESH: StringId = StringId(92);
    pub const PLUGINS_REMOVE: StringId = StringId(93);
    // 94..99 keybindings modal
    pub const KEYBINDINGS_TITLE: StringId = StringId(94);
    pub const KEYBINDINGS_RECORDING: StringId = StringId(95);
    pub const KEYBINDINGS_UNSUPPORTED: StringId = StringId(96);
    pub const KEYBINDINGS_RECORD: StringId = StringId(97);
    pub const KEYBINDINGS_RESET: StringId = StringId(98);
    // 99 reserved

    // 100..111 Wave J1 — theme picker (10 built-in theme presets + picker chrome)
    // Note: preset 0 ("默认") reuses the existing `THEME_DEFAULT` id (87); the
    // 9 ids below name the remaining 9 built-in presets, plus the accent-row
    // label and the Save button used by the picker's footer.
    pub const THEME_DAYLIGHT: StringId = StringId(100);
    pub const THEME_SUNSET: StringId = StringId(101);
    pub const THEME_OCEAN: StringId = StringId(102);
    pub const THEME_FOREST: StringId = StringId(103);
    pub const THEME_LAVENDER: StringId = StringId(104);
    pub const THEME_ROSE: StringId = StringId(105);
    pub const THEME_MIDNIGHT: StringId = StringId(106);
    pub const THEME_MONOCHROME: StringId = StringId(107);
    pub const THEME_EMBER: StringId = StringId(108);
    /// Theme picker — "强调色" / "Accent color" row label.
    pub const THEME_PICKER_ACCENT: StringId = StringId(109);
    /// Theme picker — "保存" / "Save" footer button.
    pub const BTN_SAVE: StringId = StringId(110);

    // Wave K1/K2 — Settings panel locale dropdown labels. Both locales render
    // the language name in its native script so the active-locale row reads
    // "Language: 中文" (zh-CN) or "Language: English" (en-US) directly.
    /// Locale display label for zh-CN ("中文").
    pub const LOCALE_LABEL_ZH_CN: StringId = StringId(111);
    /// Locale display label for en-US ("English").
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

    // 121..127 Round-2 M2 — Settings 桌面源/桌面路径/监控值 labels.
    pub const SECTION_DESKTOP_SOURCES: StringId = StringId(121);
    pub const SECTION_DESKTOP_PATH: StringId = StringId(122);
    pub const SECTION_WATCH_VALUES: StringId = StringId(123);
    pub const SOURCE_PRIMARY_LABEL: StringId = StringId(124);
    pub const SOURCE_PUBLIC_LABEL: StringId = StringId(125);
    pub const WATCH_HINT_LINE_EACH: StringId = StringId(126);

    // 127..138 M1d (2026-05-29) — orphan slots. The bespoke 高级 +
    // 未来集成验证 sections (nano-only, absent from Tauri) were deleted in
    // M1d; their 11 ids 127..=137 are blanked to "" in BOTH tables (slots
    // kept so `lookup_tables_have_matching_length` + every id 0..N reference
    // stays stable) and their `pub const` names removed (no live reference).
    pub const STATE_ENABLED_PILL: StringId = StringId(138);
    pub const STATE_DISABLED_PILL: StringId = StringId(139);
    // 140 Wave I-α / R14 (2026-05-25) — row caption for the 3-radio
    // zone-display-mode picker. Separate from `ZONE_MODE_*` (77..79) so the
    // picker can render `默认显示模式  ○ 悬停  ○ 常显  ● 点击` without
    // duplicating the "模式:" prefix on every radio.
    pub const SETTINGS_ZONE_DISPLAY_MODE_LABEL: StringId = StringId(140);

    // 141 M1a (2026-05-29) — Tauri 1:1 parity, General section row 5.
    // Replaces the bespoke nano "使用模式 (速度模式)" (id 117) with the real
    // Tauri "便携模式 (需要重启)" / "Portable Mode (restart required)" label
    // bound to `setting_portable_mode`. Id 117 stays in the table (orphan)
    // to preserve numbering lockstep with `i18n_en_us`; readers must
    // reference 141 going forward.
    pub const SETTING_PORTABLE_MODE: StringId = StringId(141);

    // 142..156 M1d (2026-05-29) — Performance §5 + Startup management §6.
    // Appended in lockstep with `i18n_en_us` (same index, same order) per the
    // positional-array contract. These replace the deleted 高级/未来集成验证
    // strings with the genuine Tauri sections (`SettingsPanel.tsx:601-698`).
    // Performance group (142..146).
    pub const SETTINGS_GROUP_PERFORMANCE: StringId = StringId(142);
    pub const SETTING_EXPAND_DELAY: StringId = StringId(143);
    pub const SETTING_COLLAPSE_DELAY: StringId = StringId(144);
    pub const SETTING_ICON_CACHE_SIZE: StringId = StringId(145);
    // Startup management group (146..156).
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

    // 156..170 M1e (2026-05-29) — Stealth §7 card (`StealthModeCard.tsx`).
    // Appended in lockstep with `i18n_en_us` (same index, same order) per the
    // positional-array contract. Mirrors the Tauri `stealth*` keys
    // (`src/i18n/locales/zh-CN.ts:223-239`). The status pill reuses the same
    // applied/pending/failed derivation as Tauri `deriveLevel`.
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
    // Appended in lockstep with `i18n_en_us` (same index, same order) per the
    // positional-array contract. Mirrors the Tauri `updater*` keys
    // (`src/i18n/locales/zh-CN.ts:313-333`). The status pill maps each of the
    // nano `SettingsUpdaterStatus` variants to one of these labels; the action
    // buttons reuse the `can_run_update_action` / `can_skip_update` helpers for
    // visibility.
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
    // Appended in lockstep with `i18n_en_us` (same index, same order) per the
    // positional-array contract. Mirrors the Tauri `backup*` keys
    // (`src/i18n/locales/zh-CN.ts:336-344`). `BACKUP_REFRESH` has no Tauri
    // string key (Tauri refreshes on mount + on the `backupCreated` event); the
    // nano shell has no event listener, so it exposes an explicit Refresh
    // button that fires `ListSettingsBackups` — the nano equivalent of Tauri's
    // auto-refresh.
    pub const BACKUP_CARD_TITLE: StringId = StringId(191);
    pub const BACKUP_CARD_DESCRIPTION: StringId = StringId(192);
    pub const BACKUP_CREATE_NOW: StringId = StringId(193);
    pub const BACKUP_REFRESH: StringId = StringId(194);
    pub const BACKUP_RESTORE: StringId = StringId(195);
    pub const BACKUP_EMPTY: StringId = StringId(196);

    // 197..203 M1h (2026-05-29) — Plugins §11 inline section
    // (`SettingsPanel.tsx:709-781`). Appended in lockstep with `i18n_en_us`
    // (same index, same order) per the positional-array contract. Mirrors the
    // Tauri plugin keys (`src/i18n/locales/zh-CN.ts:208-217`). The group title
    // reuses `SETTINGS_PLUGINS` (36, "插件"); the install button gets its own
    // string ("安装插件...") distinct from the generic `BTN_INSTALL` (72,
    // "安装"), and the empty-state gets a short Tauri-matching line ("暂无已安装
    // 插件") distinct from the long drag-hint `PLUGINS_EMPTY_HINT` (91). The
    // three type badges + the per-row Uninstall button complete the 1:1 set.
    pub const PLUGIN_INSTALL: StringId = StringId(197);
    pub const PLUGIN_EMPTY: StringId = StringId(198);
    pub const PLUGIN_TYPE_THEME: StringId = StringId(199);
    pub const PLUGIN_TYPE_WIDGET: StringId = StringId(200);
    pub const PLUGIN_TYPE_ORGANIZER: StringId = StringId(201);
    pub const PLUGIN_UNINSTALL: StringId = StringId(202);
    // 203..205 M1i (2026-05-29) — Paths §2 dynamic desktop-source list
    // (`SettingsPanel.tsx:320-362`). Appended in lockstep with `i18n_en_us`
    // (same index, same order) per the positional-array contract; the
    // `lookup_tables_have_matching_length` test enforces parity. The User /
    // Public card labels reuse `SOURCE_PRIMARY_LABEL` (124) / `SOURCE_PUBLIC_LABEL`
    // (125); these add the OneDrive + Custom kind labels and the 已监视 badge
    // (Tauri keys `desktopSourceOneDrive` / `desktopSourceCustom` /
    // `desktopSourceWatched`, `zh-CN.ts:30-32`).
    pub const SOURCE_ONEDRIVE_LABEL: StringId = StringId(203);
    pub const SOURCE_CUSTOM_LABEL: StringId = StringId(204);
    pub const SOURCE_WATCHED_BADGE: StringId = StringId(205);
    // 206 M1i fidelity (2026-05-29) — `.desktop-source-empty` placeholder for
    // the §2 list when no desktop sources resolve. Tauri reuses its
    // `settingsDesktopPathPlaceholder` ("C:\Users\...\Desktop") path-format
    // hint here; nano has no existing equivalent string, so this id is appended
    // in lockstep with `i18n_en_us`.
    pub const SOURCE_EMPTY_PLACEHOLDER: StringId = StringId(206);
}

pub static ZH_CN: LookupTable = LookupTable {
    entries: &[
        // 0..2 application core
        "BentoDesk",
        "桌面整理器",
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
        "钉住",
        "取消钉住",
        "设置",
        "退出",
        "隐藏",
        "显示",
        // 16..20 reserved
        "",
        "",
        "",
        "",
        // 20..29 settings panel labels
        "设置",
        "主题",
        "快捷键",
        "语言",
        "开机自启",
        "切换",  // SETTINGS_SWITCH (25)
        "关闭",  // SETTINGS_CLOSE (26)
        "显示",  // MENU_SHOW (27)
        "退出",  // MENU_EXIT (28)
        // 29..38 G3 settings rows
        "更新",                                       // SETTINGS_UPDATES (29)
        "自动下载",                                   // SETTINGS_AUTO_DOWNLOAD (30)
        "隐身存储",                                   // SETTINGS_STEALTH_STORAGE (31)
        "配置加密",                                   // SETTINGS_VAULT_ENCRYPTION (32)
        "主题",                                       // SETTINGS_THEME_HEADING (33)
        "配置",                                       // SETTINGS_VAULT (34)
        "快捷键",                                     // SETTINGS_KEYS (35)
        "插件",                                       // SETTINGS_PLUGINS (36)
        "配置库已持久化设置；备份列表/恢复使用真实文件", // SETTINGS_PERSISTENCE_HINT (37)
        // 38..40 reserved
        "",
        "",
        // 40..43 errors
        "图形设备已断开，请重启",
        "文件未找到",
        "权限不足",
        // 43..50 reserved
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        // 50..52 status
        "加载中…",
        "就绪",
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
        "检查",   // BTN_CHECK (60)
        "开",     // BTN_ON (61)
        "关",     // BTN_OFF (62)
        "跳过",   // BTN_SKIP (63)
        "导入",   // BTN_IMPORT (64)
        "备份",   // BTN_BACKUP (65)
        "列表",   // BTN_LIST (66)
        "恢复",   // BTN_RESTORE (67)
        "打包",   // BTN_BUNDLE (68)
        "诊断",   // BTN_DIAG (69)
        "还原",   // BTN_RECOVER (70)
        "下载",   // BTN_DOWNLOAD (71)
        "安装",   // BTN_INSTALL (72)
        "等待",   // BTN_WAIT (73)
        // 74..77 updater frequency
        "每日",   // UPDATE_FREQ_DAILY (74)
        "每周",   // UPDATE_FREQ_WEEKLY (75)
        "手动",   // UPDATE_FREQ_MANUAL (76)
        // 77..80 zone display mode
        "悬停",       // ZONE_MODE_HOVER (77)
        "常显",       // ZONE_MODE_ALWAYS (78)
        "点击",       // ZONE_MODE_CLICK (79)
        // 80..87 encryption states / modes
        "输入密码",   // ENCRYPTION_TYPE_PASS (80)
        "回车解锁",   // ENCRYPTION_ENTER_UNLOCK (81)
        "回车设置",   // ENCRYPTION_ENTER_TO_SET (82)
        "解锁",       // ENCRYPTION_UNLOCK (83)
        "无",         // ENCRYPTION_MODE_NONE (84)
        "Dpapi",      // ENCRYPTION_MODE_DPAPI (85) — Windows API name; keep ASCII
        "密码",       // ENCRYPTION_MODE_PASSPHRASE (86)
        // 87 theme placeholder
        "默认",       // THEME_DEFAULT (87)
        // 88..90 updater summary tokens
        "空闲",       // UPDATER_IDLE (88)
        "检查中",     // UPDATER_CHECKING (89)
        // 90..94 plugins modal
        "基于注册表的生命周期",                                                                     // PLUGINS_REGISTRY_HINT (90)
        "尚未安装插件。将解压后的插件拖入 app-data/plugins，或点击「安装」以查看真实归档导入反馈。", // PLUGINS_EMPTY_HINT (91)
        "刷新",                                                                                     // PLUGINS_REFRESH (92)
        "卸载",                                                                                     // PLUGINS_REMOVE (93)
        // 94..99 keybindings modal
        "快捷键",     // KEYBINDINGS_TITLE (94)
        "录制中...",  // KEYBINDINGS_RECORDING (95)
        "未支持",     // KEYBINDINGS_UNSUPPORTED (96)
        "录制",       // KEYBINDINGS_RECORD (97)
        "重置",       // KEYBINDINGS_RESET (98)
        // 99 reserved
        "",
        // 100..111 Wave J1 — theme picker preset names + picker chrome
        "白昼",       // THEME_DAYLIGHT (100)
        "晚霞",       // THEME_SUNSET (101)
        "海洋",       // THEME_OCEAN (102)
        "森林",       // THEME_FOREST (103)
        "薰衣草",     // THEME_LAVENDER (104)
        "玫瑰",       // THEME_ROSE (105)
        "午夜",       // THEME_MIDNIGHT (106)
        "单色",       // THEME_MONOCHROME (107)
        "余烬",       // THEME_EMBER (108)
        "强调色",     // THEME_PICKER_ACCENT (109)
        "保存",       // BTN_SAVE (110)
        // 111..113 Wave K1/K2 — locale dropdown labels (native names)
        "中文",       // LOCALE_LABEL_ZH_CN (111)
        "English",    // LOCALE_LABEL_EN_US (112)
        // 113..121 Round-2 M1 — Settings dark shell top-section + footer.
        "桌面嵌入设",            // SETTING_DESKTOP_EMBED (113)
        "开机启动",              // SETTING_AUTOSTART (114)
        "显示在任务栏",          // SETTING_SHOW_IN_TASKBAR (115)
        // M1a 2026-05-29 — text retargeted from "智能自动布局" to Tauri
        // "智能自动分组" (`zh-CN.ts` `settingsAutoGroup:22`). Const name kept
        // for minimal blast radius — readers bind via id 116 regardless of
        // the const's English wording.
        "智能自动分组",          // SETTING_SMART_LAYOUT (116)
        // M1a 2026-05-29 — orphan slot. The bespoke "速度模式" was replaced
        // by Tauri "便携模式" at id 141; this row stays to keep zh-CN/en-US
        // lengths in lockstep. No live code references id 117.
        "",                       // SETTING_SPEED_MODE (117) — orphan
        "语言 / Language",       // SETTING_LANGUAGE (118)
        "取消",                  // SETTING_CANCEL (119)
        "保存",                  // SETTING_SAVE (120)
        // 121..127 Round-2 M2 — 桌面源/桌面路径/监控值
        "桌面源",                // SECTION_DESKTOP_SOURCES (121)
        "桌面路径",              // SECTION_DESKTOP_PATH (122)
        "监控值 (每行一个)",     // SECTION_WATCH_VALUES (123)
        "海桌面",                // SOURCE_PRIMARY_LABEL (124)
        "公共桌面",              // SOURCE_PUBLIC_LABEL (125)
        "每行一个",              // WATCH_HINT_LINE_EACH (126)
        // 127..138 M1d (2026-05-29) — orphan slots. The bespoke 高级 +
        // 未来集成验证 sections were deleted; their 11 strings are blanked to
        // "" to keep zh-CN/en-US index lockstep. No live code references
        // ids 127..=137.
        "", // (127) — orphan (was SECTION_ADVANCED)
        "", // (128) — orphan
        "", // (129) — orphan
        "", // (130) — orphan
        "", // (131) — orphan
        "", // (132) — orphan
        "", // (133) — orphan
        "", // (134) — orphan
        "", // (135) — orphan
        "", // (136) — orphan
        "", // (137) — orphan
        "已启用",                // STATE_ENABLED_PILL (138)
        "未启用",                // STATE_DISABLED_PILL (139)
        // 140 Wave I-α / R14 — picker row caption.
        "默认显示模式",          // SETTINGS_ZONE_DISPLAY_MODE_LABEL (140)
        // 141 M1a (2026-05-29) — General section row 5 (Tauri parity).
        "便携模式 (需要重启)",   // SETTING_PORTABLE_MODE (141)
        // 142..146 M1d — Performance §5 (`SettingsPanel.tsx:601-631`).
        "性能",                  // SETTINGS_GROUP_PERFORMANCE (142)
        "展开延迟",              // SETTING_EXPAND_DELAY (143)
        "收起延迟",              // SETTING_COLLAPSE_DELAY (144)
        "图标缓存大小",          // SETTING_ICON_CACHE_SIZE (145)
        // 146..156 M1d — Startup management §6 (`SettingsPanel.tsx:634-698`).
        "启动管理",              // SETTINGS_GROUP_STARTUP (146)
        "高优先级启动",          // SETTING_STARTUP_HIGH_PRIORITY (147)
        "以高优先级启动以获得更快的响应", // SETTING_STARTUP_HIGH_PRIORITY_DESC (148)
        "崩溃自动重启",          // SETTING_CRASH_RESTART (149)
        "崩溃后自动重新启动应用", // SETTING_CRASH_RESTART_DESC (150)
        "最大重试次数",          // SETTING_CRASH_MAX_RETRIES (151)
        "崩溃窗口（秒）",        // SETTING_CRASH_WINDOW_SECS (152)
        "休眠安全恢复",          // SETTING_SAFE_START_HIBERNATION (153)
        "从休眠恢复后安全启动",   // SETTING_SAFE_START_HIBERNATION_DESC (154)
        "恢复延迟",              // SETTING_HIBERNATE_DELAY (155)
        // 156..170 M1e — Stealth §7 (`StealthModeCard.tsx`, zh-CN.ts:223-239).
        "桌面隐形模式",          // STEALTH_GROUP_TITLE (156)
        "状态",                  // STEALTH_STATUS_LABEL (157)
        "已应用",                // STEALTH_STATUS_APPLIED (158)
        "待重试",                // STEALTH_STATUS_PENDING (159)
        "失败",                  // STEALTH_STATUS_FAILED (160)
        "架构版本",              // STEALTH_SCHEMA_VERSION (161)
        "镜像备份",              // STEALTH_MIRROR_HEALTHY (162)
        "同步",                  // STEALTH_MIRROR_HEALTHY_YES (163)
        "待修复",                // STEALTH_MIRROR_HEALTHY_NO (164)
        "待重试项",              // STEALTH_RETRY_COUNT (165)
        "最近错误",              // STEALTH_LAST_ERROR (166)
        "刷新状态",              // STEALTH_REFRESH_BTN (167)
        "重新应用",              // STEALTH_REAPPLY_BTN (168)
        "检测到您的桌面在 OneDrive 内。建议在 OneDrive 设置中排除 .bentodesk/，否则隐藏文件会被上传到云端。", // STEALTH_ONEDRIVE_WARNING (169)
        // 170..187 M1f — Updater §8 (`UpdaterCard.tsx`, zh-CN.ts:313-333).
        "应用更新",              // UPDATER_CARD_TITLE (170)
        "状态",                  // UPDATER_STATUS_LABEL (171)
        "已是最新",              // UPDATER_STATUS_IDLE (172)
        "检查中",                // UPDATER_STATUS_CHECKING (173)
        "已是最新",              // UPDATER_STATUS_UP_TO_DATE (174)
        "有可用更新",            // UPDATER_STATUS_AVAILABLE (175)
        "下载中",                // UPDATER_STATUS_DOWNLOADING (176)
        "准备安装",              // UPDATER_STATUS_READY (177)
        "安装中",                // UPDATER_STATUS_INSTALLING (178)
        "已跳过",                // UPDATER_STATUS_SKIPPED (179)
        "错误",                  // UPDATER_STATUS_ERROR (180)
        "可用版本",              // UPDATER_AVAILABLE_VERSION (181)
        "检查更新",              // UPDATER_CHECK_NOW (182)
        "下载",                  // UPDATER_DOWNLOAD (183)
        "跳过此版本",            // UPDATER_SKIP_VERSION (184)
        "安装并重启",            // UPDATER_INSTALL_RESTART (185)
        "检查频率",              // UPDATER_FREQUENCY (186)
        "每天",                  // UPDATER_FREQ_DAILY (187)
        "每周",                  // UPDATER_FREQ_WEEKLY (188)
        "仅手动",                // UPDATER_FREQ_MANUAL (189)
        "后台静默下载",          // UPDATER_AUTO_DOWNLOAD (190)
        // 191..197 M1g — Backup §9 (`BackupCard.tsx`, zh-CN.ts:336-344).
        "设置备份",              // BACKUP_CARD_TITLE (191)
        "每次启动前自动保留最近 3 份 settings.json 备份，迁移失败可随时回滚。", // BACKUP_CARD_DESCRIPTION (192)
        "立即备份",              // BACKUP_CREATE_NOW (193)
        "刷新",                  // BACKUP_REFRESH (194)
        "恢复",                  // BACKUP_RESTORE (195)
        "暂无备份",              // BACKUP_EMPTY (196)
        // 197..203 M1h — Plugins §11 (`SettingsPanel.tsx:709-781`,
        // zh-CN.ts:208-217).
        "安装插件...",           // PLUGIN_INSTALL (197)
        "暂无已安装插件",        // PLUGIN_EMPTY (198)
        "主题",                  // PLUGIN_TYPE_THEME (199)
        "组件",                  // PLUGIN_TYPE_WIDGET (200)
        "整理器",                // PLUGIN_TYPE_ORGANIZER (201)
        "卸载",                  // PLUGIN_UNINSTALL (202)
        // 203..205 M1i — Paths §2 dynamic desktop-source list
        // (`SettingsPanel.tsx:320-362`, zh-CN.ts:30-32).
        "OneDrive 桌面",         // SOURCE_ONEDRIVE_LABEL (203)
        "自定义来源",            // SOURCE_CUSTOM_LABEL (204)
        "已监视",                // SOURCE_WATCHED_BADGE (205)
        // 206 M1i fidelity — empty `.desktop-source-empty` placeholder; mirrors
        // Tauri's reuse of the desktop-path format hint.
        "C:\\Users\\...\\Desktop", // SOURCE_EMPTY_PLACEHOLDER (206)
    ],
};
