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

    // 127..139 Round-2 M3 — 中段 advanced section + 重叠版本 + 装备状态.
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
    // 140 Wave I-α / R14 (2026-05-25) — row caption for the 3-radio
    // zone-display-mode picker. Separate from `ZONE_MODE_*` (77..79) so the
    // picker can render `默认显示模式  ○ 悬停  ○ 常显  ● 点击` without
    // duplicating the "模式:" prefix on every radio.
    pub const SETTINGS_ZONE_DISPLAY_MODE_LABEL: StringId = StringId(140);
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
        "智能自动布局",          // SETTING_SMART_LAYOUT (116)
        "使用模式 (速度模式)",   // SETTING_SPEED_MODE (117)
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
        // 127..139 Round-2 M3 — 中段 advanced section + 重叠版本 + 装备状态.
        "高级",                  // SECTION_ADVANCED (127)
        "高级洗脑启动",          // ROW_ADVANCED_STARTUP (128)
        "磁吸切换提示",          // ROW_MAGNET_SWITCH_HINT (129)
        "最大磁吸次数",          // ROW_MAX_MAGNET_COUNT (130)
        "磁吸时间 (秒)",         // ROW_MAGNET_DURATION (131)
        "快捷区分布段",          // ROW_ZONE_LAYOUT_SECTION (132)
        "致敬时长",              // ROW_BAR_COUNT_DISPLAY (133)
        "未来集成验证",          // SECTION_OVERLAY_VERSION (134)
        "架构版本",              // ROW_OVERLAY_VERSION (135)
        "装备状态",              // ROW_EQUIPMENT_STATE (136)
        "磁吸状态",              // ROW_MAGNET_STATE (137)
        "已启用",                // STATE_ENABLED_PILL (138)
        "未启用",                // STATE_DISABLED_PILL (139)
        // 140 Wave I-α / R14 — picker row caption.
        "默认显示模式",          // SETTINGS_ZONE_DISPLAY_MODE_LABEL (140)
    ],
};
