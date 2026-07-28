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

pub mod ids;

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
        "切换", // SETTINGS_SWITCH (25)
        "关闭", // SETTINGS_CLOSE (26)
        "显示", // MENU_SHOW (27)
        "退出", // MENU_EXIT (28)
        // 29..38 G3 settings rows
        "更新",                                          // SETTINGS_UPDATES (29)
        "自动下载",                                      // SETTINGS_AUTO_DOWNLOAD (30)
        "隐身存储",                                      // SETTINGS_STEALTH_STORAGE (31)
        "配置加密",                                      // SETTINGS_VAULT_ENCRYPTION (32)
        "主题",                                          // SETTINGS_THEME_HEADING (33)
        "配置",                                          // SETTINGS_VAULT (34)
        "快捷键",                                        // SETTINGS_KEYS (35)
        "插件",                                          // SETTINGS_PLUGINS (36)
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
        "检查", // BTN_CHECK (60)
        "开",   // BTN_ON (61)
        "关",   // BTN_OFF (62)
        "跳过", // BTN_SKIP (63)
        "导入", // BTN_IMPORT (64)
        "备份", // BTN_BACKUP (65)
        "列表", // BTN_LIST (66)
        "恢复", // BTN_RESTORE (67)
        "打包", // BTN_BUNDLE (68)
        "诊断", // BTN_DIAG (69)
        "还原", // BTN_RECOVER (70)
        "下载", // BTN_DOWNLOAD (71)
        "安装", // BTN_INSTALL (72)
        "等待", // BTN_WAIT (73)
        // 74..77 updater frequency
        "每日", // UPDATE_FREQ_DAILY (74)
        "每周", // UPDATE_FREQ_WEEKLY (75)
        "手动", // UPDATE_FREQ_MANUAL (76)
        // 77..80 zone display mode
        "鼠标悬停展开（默认）",  // ZONE_MODE_HOVER (77)
        "常驻展开（类 Fences）", // ZONE_MODE_ALWAYS (78)
        "单击展开",              // ZONE_MODE_CLICK (79)
        // 80..87 encryption states / modes
        "输入密码",      // ENCRYPTION_TYPE_PASS (80)
        "回车解锁",      // ENCRYPTION_ENTER_UNLOCK (81)
        "回车设置",      // ENCRYPTION_ENTER_TO_SET (82)
        "解锁",          // ENCRYPTION_UNLOCK (83)
        "无",            // ENCRYPTION_MODE_NONE (84)
        "Windows DPAPI", // ENCRYPTION_MODE_DPAPI (85)
        "密码",          // ENCRYPTION_MODE_PASSPHRASE (86)
        // 87 theme placeholder
        "默认", // THEME_DEFAULT (87)
        // 88..90 updater summary tokens
        "空闲",   // UPDATER_IDLE (88)
        "检查中", // UPDATER_CHECKING (89)
        // 90..94 plugins modal
        "基于注册表的生命周期", // PLUGINS_REGISTRY_HINT (90)
        "尚未安装插件。将解压后的插件拖入 app-data/plugins，或点击「安装」以查看真实归档导入反馈。", // PLUGINS_EMPTY_HINT (91)
        "刷新", // PLUGINS_REFRESH (92)
        "卸载", // PLUGINS_REMOVE (93)
        // 94..99 keybindings modal
        "快捷键",    // KEYBINDINGS_TITLE (94)
        "录制中...", // KEYBINDINGS_RECORDING (95)
        "未支持",    // KEYBINDINGS_UNSUPPORTED (96)
        "录制",      // KEYBINDINGS_RECORD (97)
        "重置",      // KEYBINDINGS_RESET (98)
        // 99 reserved
        "",
        // 100..111 Wave J1 — theme picker preset names + picker chrome
        "白昼",   // THEME_DAYLIGHT (100)
        "晚霞",   // THEME_SUNSET (101)
        "海洋",   // THEME_OCEAN (102)
        "森林",   // THEME_FOREST (103)
        "薰衣草", // THEME_LAVENDER (104)
        "玫瑰",   // THEME_ROSE (105)
        "午夜",   // THEME_MIDNIGHT (106)
        "单色",   // THEME_MONOCHROME (107)
        "余烬",   // THEME_EMBER (108)
        "强调色", // THEME_PICKER_ACCENT (109)
        "保存",   // BTN_SAVE (110)
        // 111..113 Wave K1/K2 — locale dropdown labels (native names)
        "中文",    // LOCALE_LABEL_ZH_CN (111)
        "English", // LOCALE_LABEL_EN_US (112)
        // 113..121 Round-2 M1 — Settings dark shell top-section + footer.
        "桌面嵌入设",   // SETTING_DESKTOP_EMBED (113)
        "开机启动",     // SETTING_AUTOSTART (114)
        "显示在任务栏", // SETTING_SHOW_IN_TASKBAR (115)
        // M1a 2026-05-29 — text retargeted from "智能自动布局" to Tauri
        // "智能自动分组" (`zh-CN.ts` `settingsAutoGroup:22`). Const name kept
        // for minimal blast radius — readers bind via id 116 regardless of
        // the const's English wording.
        "智能自动分组", // SETTING_SMART_LAYOUT (116)
        // M1a 2026-05-29 — orphan slot. The bespoke "速度模式" was replaced
        // by Tauri "便携模式" at id 141; this row stays to keep zh-CN/en-US
        // lengths in lockstep. No live code references id 117.
        "",                // SETTING_SPEED_MODE (117) — orphan
        "语言 / Language", // SETTING_LANGUAGE (118)
        "取消",            // SETTING_CANCEL (119)
        "保存",            // SETTING_SAVE (120)
        // 121..127 Round-2 M2 — 桌面源/桌面路径/监控值
        "桌面源",            // SECTION_DESKTOP_SOURCES (121)
        "桌面路径",          // SECTION_DESKTOP_PATH (122)
        "监控值 (每行一个)", // SECTION_WATCH_VALUES (123)
        "海桌面",            // SOURCE_PRIMARY_LABEL (124)
        "公共桌面",          // SOURCE_PUBLIC_LABEL (125)
        "每行一个",          // WATCH_HINT_LINE_EACH (126)
        // 127..138 M1d (2026-05-29) — orphan slots. The bespoke 高级 +
        // 未来集成验证 sections were deleted; their 11 strings are blanked to
        // "" to keep zh-CN/en-US index lockstep. No live code references
        // ids 127..=137.
        "",       // (127) — orphan (was SECTION_ADVANCED)
        "",       // (128) — orphan
        "",       // (129) — orphan
        "",       // (130) — orphan
        "",       // (131) — orphan
        "",       // (132) — orphan
        "",       // (133) — orphan
        "",       // (134) — orphan
        "",       // (135) — orphan
        "",       // (136) — orphan
        "",       // (137) — orphan
        "已启用", // STATE_ENABLED_PILL (138)
        "未启用", // STATE_DISABLED_PILL (139)
        // 140 Wave I-α / R14 — picker row caption.
        "默认显示模式", // SETTINGS_ZONE_DISPLAY_MODE_LABEL (140)
        // 141 M1a (2026-05-29) — General section row 5 (Tauri parity).
        "便携模式 (需要重启)", // SETTING_PORTABLE_MODE (141)
        // 142..146 M1d — Performance §5 (`SettingsPanel.tsx:601-631`).
        "性能",         // SETTINGS_GROUP_PERFORMANCE (142)
        "展开延迟",     // SETTING_EXPAND_DELAY (143)
        "收起延迟",     // SETTING_COLLAPSE_DELAY (144)
        "图标缓存大小", // SETTING_ICON_CACHE_SIZE (145)
        // 146..156 M1d — Startup management §6 (`SettingsPanel.tsx:634-698`).
        "启动管理",                       // SETTINGS_GROUP_STARTUP (146)
        "高优先级启动",                   // SETTING_STARTUP_HIGH_PRIORITY (147)
        "以高优先级启动以获得更快的响应", // SETTING_STARTUP_HIGH_PRIORITY_DESC (148)
        "崩溃自动重启",                   // SETTING_CRASH_RESTART (149)
        "崩溃后自动重新启动应用",         // SETTING_CRASH_RESTART_DESC (150)
        "最大重试次数",                   // SETTING_CRASH_MAX_RETRIES (151)
        "崩溃窗口（秒）",                 // SETTING_CRASH_WINDOW_SECS (152)
        "休眠安全恢复",                   // SETTING_SAFE_START_HIBERNATION (153)
        "从休眠恢复后安全启动",           // SETTING_SAFE_START_HIBERNATION_DESC (154)
        "恢复延迟",                       // SETTING_HIBERNATE_DELAY (155)
        // 156..170 M1e — Stealth §7 (`StealthModeCard.tsx`, zh-CN.ts:223-239).
        "桌面隐形模式", // STEALTH_GROUP_TITLE (156)
        "状态",         // STEALTH_STATUS_LABEL (157)
        "已应用",       // STEALTH_STATUS_APPLIED (158)
        "待重试",       // STEALTH_STATUS_PENDING (159)
        "失败",         // STEALTH_STATUS_FAILED (160)
        "架构版本",     // STEALTH_SCHEMA_VERSION (161)
        "镜像备份",     // STEALTH_MIRROR_HEALTHY (162)
        "同步",         // STEALTH_MIRROR_HEALTHY_YES (163)
        "待修复",       // STEALTH_MIRROR_HEALTHY_NO (164)
        "待重试项",     // STEALTH_RETRY_COUNT (165)
        "最近错误",     // STEALTH_LAST_ERROR (166)
        "刷新状态",     // STEALTH_REFRESH_BTN (167)
        "重新应用",     // STEALTH_REAPPLY_BTN (168)
        "检测到您的桌面在 OneDrive 内。建议在 OneDrive 设置中排除 .bentodesk/，否则隐藏文件会被上传到云端。", // STEALTH_ONEDRIVE_WARNING (169)
        // 170..187 M1f — Updater §8 (`UpdaterCard.tsx`, zh-CN.ts:313-333).
        "应用更新",     // UPDATER_CARD_TITLE (170)
        "状态",         // UPDATER_STATUS_LABEL (171)
        "已是最新",     // UPDATER_STATUS_IDLE (172)
        "检查中",       // UPDATER_STATUS_CHECKING (173)
        "已是最新",     // UPDATER_STATUS_UP_TO_DATE (174)
        "有可用更新",   // UPDATER_STATUS_AVAILABLE (175)
        "下载中",       // UPDATER_STATUS_DOWNLOADING (176)
        "准备安装",     // UPDATER_STATUS_READY (177)
        "安装中",       // UPDATER_STATUS_INSTALLING (178)
        "已跳过",       // UPDATER_STATUS_SKIPPED (179)
        "错误",         // UPDATER_STATUS_ERROR (180)
        "可用版本",     // UPDATER_AVAILABLE_VERSION (181)
        "检查更新",     // UPDATER_CHECK_NOW (182)
        "下载",         // UPDATER_DOWNLOAD (183)
        "跳过此版本",   // UPDATER_SKIP_VERSION (184)
        "安装并重启",   // UPDATER_INSTALL_RESTART (185)
        "检查频率",     // UPDATER_FREQUENCY (186)
        "每天",         // UPDATER_FREQ_DAILY (187)
        "每周",         // UPDATER_FREQ_WEEKLY (188)
        "仅手动",       // UPDATER_FREQ_MANUAL (189)
        "后台静默下载", // UPDATER_AUTO_DOWNLOAD (190)
        // 191..197 M1g — Backup §9 (`BackupCard.tsx`, zh-CN.ts:336-344).
        "设置备份", // BACKUP_CARD_TITLE (191)
        "每次启动前自动保留最近 3 份 settings.json 备份，迁移失败可随时回滚。", // BACKUP_CARD_DESCRIPTION (192)
        "立即备份", // BACKUP_CREATE_NOW (193)
        "刷新",     // BACKUP_REFRESH (194)
        "恢复",     // BACKUP_RESTORE (195)
        "暂无备份", // BACKUP_EMPTY (196)
        // 197..203 M1h — Plugins §11 (`SettingsPanel.tsx:709-781`,
        // zh-CN.ts:208-217).
        "安装插件...",    // PLUGIN_INSTALL (197)
        "暂无已安装插件", // PLUGIN_EMPTY (198)
        "主题",           // PLUGIN_TYPE_THEME (199)
        "组件",           // PLUGIN_TYPE_WIDGET (200)
        "整理器",         // PLUGIN_TYPE_ORGANIZER (201)
        "卸载",           // PLUGIN_UNINSTALL (202)
        // 203..205 M1i — Paths §2 dynamic desktop-source list
        // (`SettingsPanel.tsx:320-362`, zh-CN.ts:30-32).
        "OneDrive 桌面", // SOURCE_ONEDRIVE_LABEL (203)
        "自定义来源",    // SOURCE_CUSTOM_LABEL (204)
        "已监视",        // SOURCE_WATCHED_BADGE (205)
        // 206 M1i fidelity — empty `.desktop-source-empty` placeholder; mirrors
        // Tauri's reuse of the desktop-path format hint.
        "C:\\Users\\...\\Desktop", // SOURCE_EMPTY_PLACEHOLDER (206)
        // 207..231 M6-UI — §3 Appearance inline theme grid
        // (`SettingsPanel.tsx:396-536`, zh-CN.ts:163-183). Group headings,
        // the 17 theme display names, the Appearance group title, the
        // theme-picker label and the accent-colour row label.
        "圆角玻璃", // THEME_GROUP_ROUNDED (207)
        "实心",     // THEME_GROUP_SOLID (208)
        "方角现代", // THEME_GROUP_ANGULAR (209)
        "个性",     // THEME_GROUP_PERSONALITY (210)
        "深色",     // THEME_NAME_DARK (211)
        "浅色",     // THEME_NAME_LIGHT (212)
        "午夜",     // THEME_NAME_MIDNIGHT (213)
        "森林",     // THEME_NAME_FOREST (214)
        "日落",     // THEME_NAME_SUNSET (215)
        "毛玻璃",   // THEME_NAME_FROSTED (216)
        "海洋蓝",   // THEME_NAME_OCEAN_BLUE (217)
        "玫瑰金",   // THEME_NAME_ROSE_GOLD (218)
        "森林绿",   // THEME_NAME_FOREST_GREEN (219)
        "纯色",     // THEME_NAME_SOLID (220)
        "秩序",     // THEME_NAME_ORDER (221)
        "扁平",     // THEME_NAME_FLAT (222)
        "粗野派",   // THEME_NAME_BRUTALISM (223)
        "杂志",     // THEME_NAME_EDITORIAL (224)
        "新拟态",   // THEME_NAME_NEO (225)
        "终端",     // THEME_NAME_TERMINAL (226)
        "赛博朋克", // THEME_NAME_CYBERPUNK (227)
        "外观",     // SETTINGS_GROUP_APPEARANCE (228)
        "选择主题", // THEME_PICKER_LABEL (229)
        "强调色",   // SETTINGS_ACCENT_COLOR (230)
        // 231..244 M7 — §10 Encryption card (`EncryptionCard.tsx`,
        // zh-CN.ts:347-361). Appended in lockstep with `i18n_en_us`.
        "设置加密", // ENCRYPTION_CARD_TITLE (231)
        "启用加密可防止 OneDrive / Google Drive 同步时泄漏敏感设置。", // ENCRYPTION_CARD_DESC (232)
        "当前模式", // ENCRYPTION_CURRENT_MODE (233)
        "默认,兼容最佳", // ENCRYPTION_MODE_NONE_SUB (234)
        "当前用户透明加密,无需密码", // ENCRYPTION_MODE_DPAPI_SUB (235)
        "自定义口令", // ENCRYPTION_MODE_PASSPHRASE_FULL (236)
        "AES-256-GCM,跨机器可解", // ENCRYPTION_MODE_PASSPHRASE_SUB (237)
        "口令",     // ENCRYPTION_PASSPHRASE_LABEL (238)
        "至少 8 位字符", // ENCRYPTION_PASSPHRASE_PLACEHOLDER (239)
        "口令不会以明文存储;丢失后无法找回,请妥善保管。", // ENCRYPTION_PASSPHRASE_HINT (240)
        "请先输入口令再切换到口令模式", // ENCRYPTION_REQUIRED (241)
        "口令校验失败", // ENCRYPTION_PROBE_FAILED (242)
        "已切换加密模式为", // ENCRYPTION_MODE_APPLIED (243)
        // 244..254 V21-T2/C44 — StackTray / FocusedZonePreview overlay.
        "项目",                               // BULK_MANAGER_COL_ITEMS (244)
        "解散堆栈",                           // STACK_DISSOLVE (245)
        "成员",                               // STACK_MEMBERS_LABEL (246)
        "拆出",                               // STACK_DETACH_MEMBER (247)
        "更多成员",                           // STACK_MORE_MEMBERS (248)
        "更多堆栈成员",                       // STACK_MORE_STACK_MEMBERS (249)
        "拖到行上以重新排序",                 // STACK_REORDER_HINT (250)
        "焦点预览",                           // FOCUSED_PREVIEW_TITLE (251)
        "此成员暂无桌面项目。",               // FOCUSED_PREVIEW_EMPTY (252)
        "·",                                  // STACK_DIMENSION_SEPARATOR (253)
        "预览中",                             // STACK_PREVIEW_ACTIVE (254)
        "通用",                               // SETTINGS_GROUP_GENERAL (255)
        "路径",                               // SETTINGS_GROUP_PATHS (256)
        "Zone 显示方式",                      // SETTINGS_GROUP_DISPLAY_MODE (257)
        "Zone 唤醒方式",                      // SETTINGS_DISPLAY_MODE_LABEL (258)
        "选择胶囊如何展开成完整 Zone 面板。", // SETTINGS_DISPLAY_MODE_HINT (259)
        // 260..292 polished native global Search surface.
        "全局搜索",                               // SEARCH_TITLE (260)
        "关闭搜索",                               // SEARCH_CLOSE (261)
        "搜索 Zone、文件、设置或操作",            // SEARCH_PLACEHOLDER (262)
        "输入关键词；使用方向键选择，回车打开。", // SEARCH_IDLE_HINT (263)
        "没有找到匹配结果",                       // SEARCH_EMPTY (264)
        "文件",                                   // SEARCH_KIND_FILE (265)
        "文件夹",                                 // SEARCH_KIND_FOLDER (266)
        "Zone",                                   // SEARCH_KIND_ZONE (267)
        "设置",                                   // SEARCH_KIND_SETTING (268)
        "操作",                                   // SEARCH_KIND_ACTION (269)
        " 个实时结果",                            // SEARCH_RESULTS_SUFFIX (270)
        "设置",                                   // SEARCH_GROUP_SETTINGS (271)
        "操作",                                   // SEARCH_GROUP_ACTIONS (272)
        "显示语言",                               // SEARCH_SETTING_LOCALE (273)
        "更新检查频率",                           // SEARCH_SETTING_UPDATE_FREQUENCY (274)
        "自动下载更新",                           // SEARCH_SETTING_AUTO_DOWNLOAD (275)
        "桌面隐形存储",                           // SEARCH_SETTING_STEALTH (276)
        "设置加密模式",                           // SEARCH_SETTING_ENCRYPTION (277)
        "Zone 显示方式",                          // SEARCH_SETTING_ZONE_DISPLAY (278)
        "快捷键",                                 // SEARCH_SETTING_KEYBINDINGS (279)
        "当前主题",                               // SEARCH_SETTING_THEME (280)
        "新建 Zone",                              // SEARCH_ACTION_CREATE_ZONE (281)
        "打开设置",                               // SEARCH_ACTION_OPEN_SETTINGS (282)
        "打开关于",                               // SEARCH_ACTION_OPEN_ABOUT (283)
        "打开时间线",                             // SEARCH_ACTION_OPEN_TIMELINE (284)
        "打开布局快照",                           // SEARCH_ACTION_OPEN_SNAPSHOTS (285)
        "打开智能分组建议",                       // SEARCH_ACTION_OPEN_SUGGESTOR (286)
        "打开批量管理",                           // SEARCH_ACTION_OPEN_BULK_MANAGER (287)
        "打开情境胶囊",                           // SEARCH_ACTION_OPEN_CAPSULE_PICKER (288)
        "打开规则向导",                           // SEARCH_ACTION_OPEN_RULES (289)
        "查看固定的迷你栏",                       // SEARCH_ACTION_LIST_MINIBARS (290)
        "切换调试信息",                           // SEARCH_ACTION_TOGGLE_DEBUG (291)
        "退出 BentoDesk",                         // SEARCH_ACTION_QUIT (292)
        "已取消安装插件",                         // PLUGIN_STATUS_INSTALL_CANCELLED (293)
        "已安装插件：",                           // PLUGIN_STATUS_INSTALLED_PREFIX (294)
        " 已启用",                                // PLUGIN_STATUS_ENABLED_SUFFIX (295)
        " 已停用",                                // PLUGIN_STATUS_DISABLED_SUFFIX (296)
        "已卸载插件：",                           // PLUGIN_STATUS_REMOVED_PREFIX (297)
        "确认卸载此插件？",                       // PLUGIN_CONFIRM_UNINSTALL (298)
        "确认",                                   // BTN_CONFIRM (299)
        "读取插件列表失败：",                     // PLUGIN_STATUS_LIST_FAILED_PREFIX (300)
        "安装插件失败：",                         // PLUGIN_STATUS_INSTALL_FAILED_PREFIX (301)
        "切换插件状态失败：",                     // PLUGIN_STATUS_TOGGLE_FAILED_PREFIX (302)
        "卸载插件失败：",                         // PLUGIN_STATUS_UNINSTALL_FAILED_PREFIX (303)
    ],
};
