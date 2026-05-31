//! Application + window state containers.
//!
//! `AppState` owns the widget tree (the cross-window domain model). Layout
//! caching is per-HWND state — Ruling 5 / C3 commitment — so the
//! `LayoutEngine` lives on `WindowState`. Today we open one window so the
//! split looks redundant, but the multi-monitor / multi-window story in
//! Phase 2.x onward will mount additional `WindowState`s against a single
//! `AppState`.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;

use bento_nano_backend::{
    desktop_sources::DesktopSourceKind, rules::Rule, updater::UpdateCheckFrequency,
};
use bento_nano_layout::{LayoutEngine, LayoutError};
use bento_nano_platform::MonitorInfo;
use bento_nano_style::Size;
use bento_nano_style::tokens::{
    PALETTE_DARK, PaletteTauri, RADIUS, RadiusTauri, SHADOW, ShadowTauri, TYPOGRAPHY,
    TypographyTauri,
};
use bento_nano_theme::{
    DARK_DEFAULT, LIGHT_DEFAULT, PaletteTokens, RadiusTokens, ShadowTokens, SpacingTokens,
    THEMES, ThemeTokens, TypoTokens,
};
use bento_nano_tree::{NodeId, Tree, TreeError};
use bento_nano_widget::WidgetNode;
use bento_nano_zone::{DEFAULT_ZONE_DISPLAY_MODE, Zone, ZoneId, ZoneItemId, ZoneList};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::{
    animator::Animator,
    business::{
        bulk_manager_panel::BulkManagerState,
        capsule_picker::CapsulePickerState,
        debug_overlay::DebugOverlayState,
        highlight_overlay::HighlightOverlayState,
        item_card::ItemHoverState,
        minibar::{MAX_MINIBARS, MiniBar},
        rules_wizard::RulesWizardState,
        search_bar::SearchBarState,
        smart_group_suggestor::SuggestorState,
        stack_tray::{StackTrayDragState, StackTrayState},
        timeline::{TimelinePanelState, snapshot_picker::SnapshotPickerState},
    },
    dispatcher::PaletteTarget,
    zone_pill_geometry::HoverScheduler,
};

// ── M1d 2026-05-29 — Performance §5 + Startup management §6 bounds ──────
// Min/max/step lifted 1:1 from Tauri `SettingsPanel.tsx:601-698`. Stored as
// `pub const` so geometry (`settings_panel.rs`), the shell dispatch
// (`main.rs`), and unit tests share one source of truth and never drift.

/// 展开延迟 / Expand Delay — `SettingsPanel.tsx:607-609`.
pub const EXPAND_DELAY_MIN_MS: i32 = 50;
pub const EXPAND_DELAY_MAX_MS: i32 = 500;
pub const EXPAND_DELAY_STEP_MS: i32 = 10;
/// 收起延迟 / Collapse Delay — `SettingsPanel.tsx:616-618`.
pub const COLLAPSE_DELAY_MIN_MS: i32 = 100;
pub const COLLAPSE_DELAY_MAX_MS: i32 = 1000;
pub const COLLAPSE_DELAY_STEP_MS: i32 = 50;
/// 图标缓存大小 / Icon Cache Size — `SettingsPanel.tsx:625-627`.
pub const ICON_CACHE_MIN: i32 = 100;
pub const ICON_CACHE_MAX: i32 = 2000;
pub const ICON_CACHE_STEP: i32 = 100;
/// 最大重试次数 / Max Retries — `SettingsPanel.tsx:657-658`.
pub const CRASH_MAX_RETRIES_MIN: i32 = 1;
pub const CRASH_MAX_RETRIES_MAX: i32 = 10;
/// 崩溃窗口（秒）/ Crash Window (s) — `SettingsPanel.tsx:670-671`.
pub const CRASH_WINDOW_SECS_MIN: i32 = 5;
pub const CRASH_WINDOW_SECS_MAX: i32 = 60;
/// 恢复延迟 / Resume Delay — `SettingsPanel.tsx:691-693`.
pub const HIBERNATE_DELAY_MIN_MS: i32 = 500;
pub const HIBERNATE_DELAY_MAX_MS: i32 = 5000;
pub const HIBERNATE_DELAY_STEP_MS: i32 = 100;

/// M1d — map a slider track fraction `[0,1]` to a stepped value in
/// `[min, max]`, snapped to `step` and clamped. Pure helper shared by the
/// drag-dispatch arms; keeps the quantization unit-testable away from the
/// shell. `step` must be > 0 (all call sites pass a positive const); a
/// non-positive step degrades to a plain clamp so the function stays
/// panic-free.
pub fn slider_fraction_to_value(frac: f32, min: i32, max: i32, step: i32) -> i32 {
    let frac = frac.clamp(0.0, 1.0);
    let raw = min as f32 + frac * (max - min) as f32;
    if step <= 0 {
        return (raw.round() as i32).clamp(min, max);
    }
    let steps = ((raw - min as f32) / step as f32).round() as i32;
    (min + steps * step).clamp(min, max)
}

/// M6a — English display name for a builtin theme id. The localized (zh)
/// names land in M6-UI alongside the theme grid; this map only supplies a
/// stable English label for the Settings active-theme row when a theme is
/// applied by id without the backend loader. Unknown ids echo the id back.
/// (No i18n table is touched — M6a adds no `StringId`.)
pub fn builtin_theme_display_name(id: &str) -> SmolStr {
    let name = match id {
        "dark" => "Dark",
        "light" => "Light",
        "midnight" => "Midnight",
        "forest" => "Forest",
        "sunset" => "Sunset",
        "frosted" => "Frosted",
        "ocean-blue" => "Ocean Blue",
        "rose-gold" => "Rose Gold",
        "forest-green" => "Forest Green",
        "solid" => "Solid",
        "order" => "Order",
        "flat" => "Flat",
        "brutalism" => "Brutalism",
        "editorial" => "Editorial",
        "neo" => "Neo",
        "terminal" => "Terminal",
        "cyberpunk" => "Cyberpunk",
        other => return SmolStr::new(other),
    };
    SmolStr::new_static(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEncryptionMode {
    None,
    Dpapi,
    Passphrase,
}

impl SettingsEncryptionMode {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Dpapi => "Dpapi",
            Self::Passphrase => "Passphrase",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassphraseEntryPurpose {
    Set,
    Unlock,
}

/// M1a 2026-05-29 — snapshot of every persisted Settings toggle captured
/// when the panel opens. Cancel/Escape/Close × replay this back onto the
/// `AppState` Cells so a mid-edit dismissal never leaks into the vault.
///
/// M1d 2026-05-29 — extended past the 5 General toggles to cover the
/// Performance (3 sliders) + Startup-management (2 toggles + 2 steppers +
/// 1 toggle + 1 slider) sections. All these fields are Save-gated (NOT
/// immediate), so Cancel must revert them; `snapshot_settings`/
/// `restore_settings` stay the single round-trip surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsSnapshot {
    pub ghost_layer_enabled: bool,
    pub launch_at_startup: bool,
    pub show_in_taskbar: bool,
    pub auto_group_enabled: bool,
    pub portable_mode: bool,
    // M1d — Performance section (§5).
    pub expand_delay_ms: i32,
    pub collapse_delay_ms: i32,
    pub icon_cache_size: i32,
    // M1d — Startup management section (§6).
    pub startup_high_priority: bool,
    pub crash_restart_enabled: bool,
    pub crash_max_retries: i32,
    pub crash_window_secs: i32,
    pub safe_start_after_hibernation: bool,
    pub hibernate_resume_delay_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsBackupStatus {
    Success(SmolStr),
    Error(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsKeybindingFeedback {
    Success { action: SmolStr, message: SmolStr },
    Error { action: SmolStr, message: SmolStr },
}

impl SettingsKeybindingFeedback {
    pub fn action(&self) -> &str {
        match self {
            Self::Success { action, .. } | Self::Error { action, .. } => action.as_str(),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Success { message, .. } | Self::Error { message, .. } => message.as_str(),
        }
    }

    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsUpdaterStatus {
    Idle,
    Checking,
    UpToDate {
        current_version: SmolStr,
    },
    Available {
        version: SmolStr,
    },
    Downloading {
        chunk_len: u64,
        total_bytes: Option<u64>,
    },
    Ready {
        version: SmolStr,
    },
    Installing {
        version: SmolStr,
    },
    Skipped {
        version: SmolStr,
    },
    Error(SmolStr),
}

impl SettingsUpdaterStatus {
    pub fn summary(&self) -> SmolStr {
        match self {
            Self::Idle => SmolStr::new_static("Idle"),
            Self::Checking => SmolStr::new_static("Checking"),
            Self::UpToDate { current_version } => {
                SmolStr::new(format!("Up to date {current_version}"))
            }
            Self::Available { version } => SmolStr::new(format!("Available {version}")),
            Self::Downloading {
                chunk_len,
                total_bytes,
            } => match total_bytes {
                Some(total) => SmolStr::new(format!("Downloading {chunk_len}/{total} B")),
                None => SmolStr::new(format!("Downloading {chunk_len} B")),
            },
            Self::Ready { version } => SmolStr::new(format!("Ready {version}")),
            Self::Installing { version } => SmolStr::new(format!("Installing {version}")),
            Self::Skipped { version } => SmolStr::new(format!("Skipped {version}")),
            Self::Error(message) => message.clone(),
        }
    }

    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub const fn is_success(&self) -> bool {
        matches!(
            self,
            Self::UpToDate { .. }
                | Self::Available { .. }
                | Self::Ready { .. }
                | Self::Installing { .. }
                | Self::Skipped { .. }
        )
    }

    pub const fn can_run_update_action(&self) -> bool {
        matches!(self, Self::Available { .. } | Self::Ready { .. })
    }

    pub const fn can_skip_update(&self) -> bool {
        matches!(self, Self::Available { .. } | Self::Ready { .. })
    }

    pub const fn action_label(&self) -> &'static str {
        match self {
            Self::Available { .. } => "Download",
            Self::Ready { .. } => "Install",
            Self::Installing { .. } => "Wait",
            Self::Downloading { .. } => "Wait",
            _ => "Download",
        }
    }

    pub fn version_for_skip(&self) -> Option<SmolStr> {
        match self {
            Self::Available { version } | Self::Ready { version } => Some(version.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsBackupEntry {
    pub id: SmolStr,
    pub file_name: SmolStr,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeOption {
    pub id: SmolStr,
    pub name: SmolStr,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsPluginEntry {
    pub id: SmolStr,
    pub name: SmolStr,
    pub version: SmolStr,
    pub plugin_type: SmolStr,
    /// M1h — plugin author from the manifest (`InstalledPlugin::author`). The
    /// inline Plugins §11 card shows this on its own line, matching Tauri
    /// `plugin-card__author` (`SettingsPanel.tsx:749`).
    pub author: SmolStr,
    /// M1h — plugin description from the manifest (`InstalledPlugin::
    /// description`). Rendered as the card's description line, matching Tauri
    /// `plugin-card__desc` (`SettingsPanel.tsx:750`).
    pub description: SmolStr,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneDisplayMode {
    Hover,
    Always,
    Click,
}

impl ZoneDisplayMode {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Always => "always",
            Self::Click => "click",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Hover => "Hover",
            Self::Always => "Always",
            Self::Click => "Click",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Hover => Self::Always,
            Self::Always => Self::Click,
            Self::Click => Self::Hover,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "hover" => Some(Self::Hover),
            "always" => Some(Self::Always),
            "click" => Some(Self::Click),
            _ => None,
        }
    }
}

impl Default for ZoneDisplayMode {
    fn default() -> Self {
        Self::parse(DEFAULT_ZONE_DISPLAY_MODE).unwrap_or(Self::Hover)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDragCandidate {
    pub zone_id: ZoneId,
    pub item_id: ZoneItemId,
    pub path: SmolStr,
    pub start_x: i32,
    pub start_y: i32,
    pub last_x: i32,
    pub last_y: i32,
    pub is_internal_dragging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneEditorSession {
    pub zone_id: ZoneId,
    pub draft_name: String,
    pub draft_icon: SmolStr,
    pub draft_accent_color: Option<SmolStr>,
    pub draft_grid_columns: u32,
    pub draft_capsule_size: SmolStr,
    pub draft_capsule_shape: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFileRenameSession {
    pub zone_id: ZoneId,
    pub item_id: ZoneItemId,
    pub draft_name: String,
    pub current_path: SmolStr,
    pub status: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPickerSession {
    pub zone_id: Option<ZoneId>,
    pub selected_icon: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalettePickerSession {
    pub target: PaletteTarget,
    pub selected_accent: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipSession {
    pub text: SmolStr,
}

#[derive(Debug)]
pub struct AppState {
    pub tree: Tree<WidgetNode>,
    /// Cached viewport size in DIPs (updated by the shell on WM_SIZE). Today
    /// matches the single window's client rect; when the second window
    /// lands, viewport moves onto `WindowState` and this field is dropped.
    pub viewport: Size,
    /// Zone collection — Ruling 2 / Phase 2 baseline. Persisted via
    /// `bento-nano-platform::storage`; renderer iterates via `zones.iter()`.
    pub zones: ZoneList,
    /// Monotonic id allocator for new zones. Starts at 1 because
    /// `ZoneId::INVALID = 0`.
    pub next_zone_id: Cell<u64>,
    /// `true` when the user has clicked the PIN button — main window stays
    /// always-on-top until clicked again. `Cell` for interior mutation
    /// during dispatcher drain (the dispatcher only borrows `&AppState`).
    pub is_pinned: Cell<bool>,
    /// `true` when the SETTINGS button has flipped the panel open. The
    /// modal overlay paints in `Renderer::draw_settings_panel`.
    pub settings_open: Cell<bool>,
    /// M1a 2026-05-29 — `true` after the user mutates any persisted Settings
    /// row in the open panel. Save dims when `false` (matches Tauri
    /// `disabled={!dirty()}` at `SettingsPanel.tsx:799`); Save/Cancel clear it.
    pub settings_dirty: Cell<bool>,
    /// M6-UI 2026-05-29 — in-flight accent-colour draft picked in the §3
    /// Appearance accent row (Control B). `Some("#rrggbb")` after a swatch
    /// click; the renderer rings the matching swatch, and Save persists it via
    /// the `accent_color` config-vault key. `None` falls back to the persisted
    /// `theme_base_accent`. Cancel clears the draft. `RefCell` because the
    /// value is an owned `SmolStr` (not `Copy`).
    pub settings_draft_accent_color: RefCell<Option<SmolStr>>,
    /// M1a 2026-05-29 — snapshot of the General-section toggle values taken
    /// when the Settings panel opens. Cancel/Escape/Close × restore from
    /// here so cancelled edits never leak into persisted state. `RefCell`
    /// (not `Cell`) because `SettingsSnapshot` carries multiple bools and is
    /// not `Copy`.
    pub settings_snapshot: RefCell<Option<SettingsSnapshot>>,
    /// Round-2 M1 — scroll offset for the Settings panel body. Clamped to
    /// `[0, body_max_scroll]` by the wheel handler. Header + footer stay
    /// sticky (not scrolled); only the body content shifts.
    pub scroll_offset_y: Cell<f32>,
    /// Round-2 M1 — top-section toggle: 桌面嵌入设. Default on.
    pub setting_desktop_embed: Cell<bool>,
    /// Round-2 M1 — top-section toggle: 开机启动. Default off.
    pub setting_autostart: Cell<bool>,
    /// Round-2 M1 — top-section toggle: 显示在任务栏. Default on.
    pub setting_show_in_taskbar: Cell<bool>,
    /// Round-2 M1 — top-section toggle: 智能自动分组 (Tauri Smart Auto Group).
    /// M1a 2026-05-29: label retargeted to Tauri parity, field name kept for
    /// minimal blast radius. Default on.
    pub setting_smart_layout: Cell<bool>,
    /// M1a 2026-05-29 — top-section toggle: 便携模式 (需要重启). Renamed from
    /// the bespoke `setting_speed_mode` so the General section reads 1:1 with
    /// Tauri (`SettingsPanel.tsx:294`, bound field `portable_mode`).
    /// Default off.
    pub setting_portable_mode: Cell<bool>,
    /// M1i 2026-05-29 — 桌面源 §2 dynamic, READ-ONLY source list. Replaces the
    /// two hardcoded cosmetic-toggle cards with the real resolved Desktop
    /// directories from `bento_nano_backend::desktop_sources::all_desktop_dirs`,
    /// each classified into a [`DesktopSourceKind`] and tagged with a `watched`
    /// flag (path present in the watch-paths draft). The shell repopulates this
    /// on Settings-open and on the Refresh button (`RefreshDesktopSources`); the
    /// renderer paints one read-only card per entry (Tauri `desktop-source-card`
    /// parity — `SettingsPanel.tsx:320-362`). `SmolStr` (not `PathBuf`) keeps the
    /// display string allocation-light per architecture §10.
    pub desktop_sources: RefCell<Vec<(DesktopSourceKind, SmolStr, bool)>>,
    /// Round-2 M2 — 桌面路径 draft string editing the user's primary desktop
    /// path. Wired to a single-line text input row. Persists on Save (M4).
    pub desktop_path_draft: RefCell<SmolStr>,
    /// Round-2 M2 — 监控值 draft multi-line buffer for the watch-paths
    /// textarea. One path per line. Persists on Save (M4).
    pub watch_paths_draft: RefCell<SmolStr>,
    /// M1d 2026-05-29 — Performance §5 slider: 展开延迟 / Expand Delay in ms
    /// (50..=500, step 10). Save-gated; reverted by Cancel. Deliberately
    /// produced here to unblock the M3 animation milestone — named exactly
    /// per `SettingsPanel.tsx:606`. Tauri default 150.
    pub expand_delay_ms: Cell<i32>,
    /// M1d — Performance §5 slider: 收起延迟 / Collapse Delay in ms
    /// (100..=1000, step 50). Tauri default 300 (`SettingsPanel.tsx:615`).
    /// Also unblocks the M3 animation milestone.
    pub collapse_delay_ms: Cell<i32>,
    /// M1d — Performance §5 slider: 图标缓存大小 / Icon Cache Size
    /// (100..=2000, step 100, no unit). Tauri default 500
    /// (`SettingsPanel.tsx:624`).
    pub icon_cache_size: Cell<i32>,
    /// M1d — Startup management §6 toggle: 高优先级启动 / High Priority
    /// Startup (always shown). Tauri default off (`SettingsPanel.tsx:639`).
    pub startup_high_priority: Cell<bool>,
    /// M1d — Startup management §6 toggle: 崩溃自动重启 / Crash Auto Restart
    /// (always shown). Gates the two crash steppers below. Tauri default on
    /// (`SettingsPanel.tsx:646`).
    pub crash_restart_enabled: Cell<bool>,
    /// M1d — Startup management §6 stepper: 最大重试次数 / Max Retries
    /// (1..=10), shown only when `crash_restart_enabled`. Tauri default 3
    /// (`SettingsPanel.tsx:659`).
    pub crash_max_retries: Cell<i32>,
    /// M1d — Startup management §6 stepper: 崩溃窗口（秒）/ Crash Window (s)
    /// (5..=60), shown only when `crash_restart_enabled`. Tauri default 60
    /// (`SettingsPanel.tsx:672`).
    pub crash_window_secs: Cell<i32>,
    /// M1d — Startup management §6 toggle: 休眠安全恢复 / Safe Start After
    /// Hibernation (always shown). Gates the hibernate slider below. Tauri
    /// default on (`SettingsPanel.tsx:682`).
    pub safe_start_after_hibernation: Cell<bool>,
    /// M1d — Startup management §6 slider: 恢复延迟 / Resume Delay in ms
    /// (500..=5000, step 100), shown only when
    /// `safe_start_after_hibernation`. Tauri default 2000
    /// (`SettingsPanel.tsx:690`).
    pub hibernate_resume_delay_ms: Cell<i32>,
    /// Updater check cadence restored from `updates.check_frequency`.
    /// Mutated only by selected-stack Settings controls that first persist
    /// through `Command::SetSetting`.
    pub update_check_frequency: Cell<UpdateCheckFrequency>,
    /// Updater auto-download preference restored from `updates.auto_download`.
    pub update_auto_download: Cell<bool>,
    /// Visible runtime updater state. Command handlers update this after
    /// calling the selected-stack backend updater; Settings renders it so
    /// check/download/install failures never disappear into logs.
    pub settings_updater_status: RefCell<SettingsUpdaterStatus>,
    /// Stealth storage master switch restored from `stealth.enabled`. When
    /// off, item import keeps the original desktop path instead of moving it
    /// into the hidden mirror.
    pub stealth_enabled: Cell<bool>,
    /// M1e (2026-05-29) — Stealth §7 card snapshot. Cached from the
    /// synchronous `bento_nano_backend::stealth::status()` probe when Settings
    /// opens and after Refresh/Reapply, so the immediate-mode paint and the
    /// shell hit-tester read one consistent snapshot (the conditional
    /// retry/error/OneDrive rows must agree between paint and click). `None`
    /// until the first refresh; the renderer falls back to a placeholder row.
    pub stealth_status: RefCell<Option<bento_nano_backend::stealth::StealthStatus>>,
    /// Config vault encryption mode restored from `encryption.mode`.
    pub encryption_mode: Cell<SettingsEncryptionMode>,
    /// `true` when the opened vault is passphrase-encrypted and still
    /// waiting for user unlock before persisted settings can be applied.
    pub passphrase_unlock_required: Cell<bool>,
    /// Visible status for Settings encryption mutations.
    pub settings_encryption_status: RefCell<Option<SettingsBackupStatus>>,
    /// `true` when Settings is currently capturing a passphrase draft.
    pub passphrase_entry_active: Cell<bool>,
    /// Whether the active passphrase draft will set a new passphrase or
    /// unlock an already encrypted vault.
    pub passphrase_entry_purpose: Cell<PassphraseEntryPurpose>,
    /// User-typed passphrase draft. Never rendered verbatim.
    pub passphrase_draft: RefCell<String>,
    /// Process-wide theme base accent restored from `theme.base_accent`.
    /// This is the selected-stack bridge for PalettePicker `ThemeBase`:
    /// the shell persists the picked swatch through the config vault, then
    /// the renderer uses this value for global accent affordances and as the
    /// default zone accent when a zone has no explicit override.
    pub theme_base_accent: RefCell<Option<SmolStr>>,
    /// Active full-theme id restored from the Tauri-compatible
    /// `active_theme` setting. Unlike `theme_base_accent`, this owns the
    /// renderer-ready token set, so Settings/zone chrome can change without
    /// falling back to a WebView/CSS bridge.
    pub active_theme_id: RefCell<SmolStr>,
    /// User-visible active theme name rendered by the Settings row.
    pub active_theme_name: RefCell<SmolStr>,
    /// Renderer-ready active theme tokens. The selected-stack shell validates
    /// a persisted theme id through `bento_nano_backend::themes`, then writes
    /// the converted tokens here only after the config-vault write succeeds.
    pub active_theme_tokens: RefCell<ThemeTokens>,
    /// M6a — renderer-ready Tauri-parity palette for the active theme. The 17
    /// builtins resolve to a byte-exact `PaletteTauri` const; custom JSON
    /// themes derive one off `active_theme_tokens`. Populated at the same
    /// `apply_active_theme` choke-point as `active_theme_tokens`, so boot
    /// restore and live `SetActiveTheme` both keep it in lockstep. Read by
    /// `active_theme_tauri()` (Copy, no per-frame alloc — §10).
    pub active_theme_tauri: RefCell<PaletteTauri>,
    /// M6b — renderer-ready Tauri-parity radius / shadow / typography for the
    /// active theme. Mirror of `active_theme_tauri`: the 17 builtins resolve to
    /// a per-theme const (`radius_tauri_for_theme` etc.); custom JSON themes
    /// fall back to the global `RADIUS`/`SHADOW`/`TYPOGRAPHY`. Repopulated at
    /// the same `apply_active_theme` choke-point so boot-restore + live
    /// `SetActiveTheme` keep them in lockstep. Read on the hot path via the
    /// `Copy`-returning `active_theme_*_tauri()` accessors (no per-frame alloc,
    /// §10). Drives per-theme corner sharpness (`order`/`flat`/`brutalism`),
    /// per-theme shadow stacks (`neo` dual / `terminal` glow), and the
    /// per-theme DirectWrite family (`terminal`→Consolas, `editorial`→Georgia).
    pub active_theme_radius_tauri: RefCell<RadiusTauri>,
    pub active_theme_shadow_tauri: RefCell<ShadowTauri>,
    pub active_theme_typography_tauri: RefCell<TypographyTauri>,
    /// Theme picker rows discovered from built-ins and `{app_data}/themes`.
    pub available_themes: RefCell<Vec<ThemeOption>>,
    /// Visible status for full-theme selection and display-mode settings.
    pub settings_theme_status: RefCell<Option<SettingsBackupStatus>>,
    /// Process default zone display mode restored from Tauri-compatible
    /// `zone_display_mode` (`hover`, `always`, `click`). Per-zone
    /// `Zone::display_mode` still overrides this when present.
    pub zone_display_mode: Cell<ZoneDisplayMode>,
    /// Last zone under the pointer. Renderer uses this for the real `hover`
    /// display-mode behaviour instead of a static label-only setting.
    pub hovered_zone: Cell<Option<ZoneId>>,
    /// Last zone clicked by the user. Renderer uses this for the real `click`
    /// display-mode behaviour.
    pub selected_zone: Cell<Option<ZoneId>>,
    /// Last visible Settings backup status. Set by the shell after a real
    /// config-vault backup/list/restore attempt; rendered by the Settings
    /// overlay so the backup row has a user-visible success/error result.
    pub settings_backup_status: RefCell<Option<SettingsBackupStatus>>,
    /// Real config-vault backup files discovered under app-data `backups/`.
    /// The shell owns file-system access and writes this list after a list,
    /// create, or restore command so the selected-stack Settings overlay can
    /// display actual backup availability without mock rows.
    pub settings_backup_entries: RefCell<Vec<SettingsBackupEntry>>,
    /// Real installed plugin rows discovered from `<state_dir>/plugins`. M1h
    /// (2026-05-29) — the plugins surface is now an inline §11 section of the
    /// scrollable Settings body (Tauri parity), not a separate modal, so the
    /// former `settings_plugins_open` modal gate was removed; these rows render
    /// inline and refresh on Settings open.
    pub settings_plugin_entries: RefCell<Vec<SettingsPluginEntry>>,
    /// Visible status for plugin list/install/toggle/uninstall actions.
    pub settings_plugin_status: RefCell<Option<SettingsBackupStatus>>,
    /// `true` when the keybindings modal is open above the Settings overlay.
    /// The modal is a native selected-stack D2D surface, not the Tauri
    /// KeybindingsSection webview.
    pub settings_keybindings_open: Cell<bool>,
    // M6-UI (2026-05-29) — the Wave J1b swatch-popup gate
    // (`theme_picker_open: Cell<bool>`) and its highlighted-swatch index
    // (`theme_picker_selected: Cell<u8>`) were removed: §3 Appearance is now an
    // always-inline grid in the scrollable Settings body. Selection drives the
    // live `active_theme_id` directly (via `apply_active_theme_by_id`), so no
    // separate popup-open / popup-selection state exists.
    /// Action currently recording the next chord.
    pub settings_keybinding_recording: RefCell<Option<SmolStr>>,
    /// Last per-action keybinding success/conflict message visible in the
    /// keybindings modal.
    pub settings_keybinding_feedback: RefCell<Option<SettingsKeybindingFeedback>>,
    /// `true` when the About surface is open. Kept separate from
    /// `settings_open` so tray About no longer piggybacks on the settings
    /// placeholder path.
    pub about_open: Cell<bool>,
    /// Runtime diagnostics HUD. The renderer records real frame timings and
    /// RSS samples here; shell commands and persisted `debug_overlay` toggle
    /// `visible`.
    pub debug_overlay: RefCell<DebugOverlayState>,
    /// Resolved `%APPDATA%\BentoDesk\zones.bin` path — computed once at
    /// startup (Ruling A says no `OnceCell`, just store the `PathBuf`).
    /// Default-empty path triggers the codec's missing-file branch and
    /// keeps unit tests independent of the real shell folder.
    pub zones_path: PathBuf,
    /// `true` when this cycle mutated `zones`; `consume_dispatcher` reads
    /// it after drain and saves once if set, then clears.
    pub dirty: Cell<bool>,
    /// In-flight drag — `Some((zone, dx, dy))` where (dx, dy) is the
    /// mouse-down point's offset from the zone's top-left in DIPs. None
    /// when no drag is active.
    pub zone_drag: Cell<Option<(ZoneId, i32, i32)>>,
    /// Mouse-down origin (logical DIP) for the in-flight zone drag, used to
    /// gate `MoveZone` behind the 4-DIP drag threshold (M4 — Tauri parity
    /// `ZONE_DRAG_THRESHOLD_PX = 4`). `Some((start_x, start_y, moved))` where
    /// `moved` latches `true` once the pointer travels past 4 DIP this
    /// gesture (one-way, mirroring Tauri's `moved` flag). `None` when no drag
    /// is armed. Set alongside `zone_drag` in `handle_lbutton_down`, cleared
    /// in `handle_lbutton_up`. `Copy`, alloc-free — matches the `zone_drag`
    /// idiom.
    pub zone_drag_origin: Cell<Option<(i32, i32, bool)>>,
    /// In-flight resize — `Some((zone, w0, h0))` where (w0, h0) is the
    /// zone's size at mouse-down (delta added each MOUSEMOVE).
    pub zone_resize: Cell<Option<(ZoneId, i32, i32)>>,
    /// Candidate for OLE drag-out from an item card. Set on mouse-down and
    /// promoted to `drag_drop::start_drag_operation` once mouse movement
    /// exceeds the shell threshold.
    pub item_drag: RefCell<Option<ItemDragCandidate>>,
    /// In-flight StackTray overlay session. `OpenStackTray` seeds this from
    /// live stack metadata, pointer/keyboard producers select a member for
    /// FocusedZonePreview, and detach/dissolve commands mutate the real
    /// `ZoneList` before this state is refreshed or cleared.
    pub stack_tray: RefCell<Option<StackTrayState>>,
    /// In-flight StackTray row drag. This stays UI-only until mouse-up maps it
    /// to `Command::ReorderStackMember`, so the domain mutation still flows
    /// through the dispatcher.
    pub stack_tray_drag: Cell<Option<StackTrayDragState>>,
    /// Active stack bloom reveal anchor. The shell resets this when pointer
    /// hover enters a different stack and the renderer uses it to decide
    /// whether bloom frames should use a time-based reveal progress.
    pub stack_bloom_anchor: Cell<Option<ZoneId>>,
    /// `GetTickCount` value captured when the current stack bloom reveal
    /// started. Stored in app state so rendering and hit-testing share the
    /// same animation phase inside the selected-stack pump.
    pub stack_bloom_started_ms: Cell<u32>,
    /// Current 0..1 reveal progress for Stack wrapper bloom frames.
    pub stack_bloom_progress: Cell<f32>,
    /// Wave G2 — capsule pill expand/shrink transition target.
    /// `Some(zone_id)` while that zone is animating from its collapsed pill
    /// chrome to its expanded body (hover enter) or back (hover leave). The
    /// renderer uses this + `zone_pill_progress` to paint an intermediate
    /// morphing rectangle for ≤ `ZONE_PILL_ANIM_DURATION_MS` instead of an
    /// instant swap. Direction is implied by `zone_pill_expanding`: `true`
    /// for collapse→expanded, `false` for expanded→collapse.
    pub zone_pill_anim_zone: Cell<Option<ZoneId>>,
    /// `GetTickCount` value captured when the current pill morph started.
    pub zone_pill_anim_started_ms: Cell<u32>,
    /// 0..1 progress along the pill morph. Ticked from the main pump.
    pub zone_pill_anim_progress: Cell<f32>,
    /// `true` when the animation is opening (pill → expanded), `false` when
    /// closing (expanded → pill). Determines which end-state is `progress=1`.
    pub zone_pill_anim_expanding: Cell<bool>,
    /// V-8 (2026-05-21) — capsule pill animator covering hover / press /
    /// expand / status-dot pulse channels. See `animator.rs` for the full
    /// channel + easing contract. The Wave G2 `zone_pill_anim_*` fields
    /// above still drive the rect/radius morph in `draw_zones`; the new
    /// animator layers hover-in/out + press feedback + the breathing pulse
    /// at paint time without mutating persisted geometry tokens.
    pub pill_animator: RefCell<Animator>,
    /// V-8 — zone currently registered as "pressed" (mouse-down inside a
    /// pill rect). Cleared on mouse-up regardless of release location so
    /// the press channel never lingers if the user drags off the pill.
    pub pill_pressed_zone: Cell<Option<ZoneId>>,
    /// A3 (2026-05-29) — pure hover-intent / grace-collapse scheduler. The
    /// shell feeds it `on_enter`/`on_leave` from the cursor stream and polls
    /// it once per frame; it defers expand by `expand_delay_ms`, holds the
    /// 550ms expand-lock so the easeOutBack overshoot can't be race-collapsed,
    /// and defers collapse by `collapse_delay_ms` so a transient leave doesn't
    /// drop the zone. `Copy` so a `Cell` keeps the hot path lock-free (§10).
    pub hover_scheduler: Cell<HoverScheduler>,
    /// M3-A2 (2026-05-29) — per-item hover / press scale animation state. The
    /// `item_card::card_scale_for` SSoT (CARD_HOVER_SCALE 1.02 / CARD_PRESS_SCALE
    /// 0.97) was authored + tested but never wired to live rendering; this is
    /// the live channel. The shell feeds it `on_hover`/`on_press`/`on_release`
    /// from the cursor stream (expanded-zone item grids only) and ticks it once
    /// per frame on the SAME `tick_pill_animator` cadence; `draw_item_card`
    /// samples `(hover_t, press_t)` per card at paint time and applies the
    /// centred scale. `Copy` so a `Cell` keeps the hot path lock-free (§10) —
    /// no per-item map, just the entering / leaving / pressed slots.
    pub item_hover: Cell<ItemHoverState>,
    /// In-flight selected-stack ZoneEditor session. Set by
    /// `Command::OpenZoneEditor`, edited by the ZoneEditor aux HWND's
    /// keyboard path, rendered by the ZoneEditor window, and saved through
    /// `RenameZone` plus the zone-appearance dispatcher commands on Enter.
    pub zone_editor: RefCell<Option<ZoneEditorSession>>,
    /// In-flight selected-stack item file rename session. Set by
    /// `Command::OpenItemFileRename`, edited by the ItemFileRename aux HWND,
    /// rendered natively, and saved through `Command::RenameItemFile`.
    pub item_file_rename: RefCell<Option<ItemFileRenameSession>>,
    /// Visible main-window item file-operation status/error text. Shell file
    /// commands write here after real filesystem operations so failures never
    /// disappear into logs.
    pub item_operation_status: RefCell<Option<SmolStr>>,
    /// In-flight IconPicker session. `OpenIconPicker` seeds this from the
    /// live target zone, the IconPicker HWND keyboard path updates the
    /// selected icon, and Enter emits `SetZoneIcon` for a valid zone target.
    pub icon_picker: RefCell<Option<IconPickerSession>>,
    /// In-flight PalettePicker session. `OpenPalettePicker` seeds this from
    /// the target state; the ZoneAccent path emits `SetZoneAccent` on Enter.
    /// Other targets render a visible unsupported message instead of faking
    /// success.
    pub palette_picker: RefCell<Option<PalettePickerSession>>,
    /// Context Capsule picker state. `OpenCapsulePicker` refreshes this from
    /// the filesystem-backed capsule directory, and keyboard commands mutate
    /// it through real capture / restore / delete dispatcher handlers.
    pub capsule_picker: RefCell<CapsulePickerState>,
    /// Bulk manager runtime state. `OpenBulkManager` seeds this from live
    /// zones; keyboard producers select rows and emit bulk hide/show/delete/
    /// move/layout commands through the dispatcher.
    pub bulk_manager: RefCell<BulkManagerState>,
    /// Visible BulkManager status/error text.
    pub bulk_manager_status: RefCell<Option<SmolStr>>,
    /// RulesWizard runtime draft. `OpenRulesWizard` resets this to a new
    /// selected-stack form and keyboard producers mutate it directly.
    pub rules_wizard: RefCell<RulesWizardState>,
    /// Persisted rules loaded from the backend `rules.json` store. Rendered by
    /// the RulesWizard aux HWND so list/create/update/delete are visible.
    pub rules_wizard_rules: RefCell<Vec<Rule>>,
    /// Cursor into `rules_wizard_rules` for keyboard edit/delete producers.
    pub rules_wizard_rule_cursor: Cell<usize>,
    /// Pending destructive delete confirmation for the selected persisted
    /// rule. The shell arms this on the first Delete key/click and clears it
    /// on selection changes or after the second matching Delete.
    pub rules_wizard_delete_confirm: RefCell<Option<SmolStr>>,
    /// Visible RulesWizard success/status text. Errors live on the wizard state
    /// itself via `RulesWizardState::set_error`.
    pub rules_wizard_status: RefCell<Option<SmolStr>>,
    /// Timeline panel state backed by the selected-stack checkpoint store.
    /// `OpenTimeline` refreshes this from disk; keyboard producers select,
    /// save, restore, undo, redo, delete, and pin real checkpoints.
    pub timeline_panel: RefCell<TimelinePanelState>,
    /// SnapshotPicker state backed by the selected-stack layout snapshot store.
    /// `OpenSnapshotPicker` refreshes this from disk; keyboard producers save,
    /// load, delete, confirm, and close real snapshots.
    pub snapshot_picker: RefCell<SnapshotPickerState>,
    /// SearchBar runtime state. `OpenSearch` shows the native Search HWND,
    /// keyboard producers update `query`, and the shell seeds `results` from
    /// the backend search index built from live zones/items/settings/actions.
    pub search_bar: RefCell<SearchBarState>,
    /// Visible SearchBar status/error text.
    pub search_status: RefCell<Option<SmolStr>>,
    /// Search/Suggestor highlight preview layer painted over real selected-stack
    /// zone/item geometry in the main HWND.
    pub highlight_overlay: RefCell<HighlightOverlayState>,
    /// Active Tooltip aux-window payload. `Command::ShowTooltip` writes this
    /// before showing `WindowKind::Tooltip`; the renderer consumes it to paint
    /// the selected-stack D2D tooltip surface from the real command payload.
    pub active_tooltip: RefCell<Option<TooltipSession>>,
    /// SmartGroupSuggestor runtime state. `ShowSuggestor` scans real Desktop
    /// sources, seeds this list from backend suggestions, and aux-window
    /// keyboard/pointer producers emit `GroupingApply` / `SuggestorDismiss`.
    pub suggestor: RefCell<SuggestorState>,
    /// Visible SmartGroupSuggestor scan/apply/dismiss status text.
    pub suggestor_status: RefCell<Option<SmolStr>>,
    /// SmartGroupSuggestor — set of suggestion ids the user has dismissed
    /// from the panel. UI-only state (no backend round-trip); persisted in
    /// the future Phase 4 once the suggestor panel mounts. Wave F1.3 wires
    /// `Command::SuggestorDismiss` to insert here so the variant has a
    /// real consumer instead of falling through to `unhandled_command_log`.
    pub suggestor_dismissed: RefCell<HashSet<SmolStr>>,
    /// Pinned MiniBar descriptors keyed by their source zone. The shell keeps
    /// the HWND lifecycle, while the renderer consumes this selected-stack
    /// state to paint the actual MiniBar surface.
    pub minibars: RefCell<SmallVec<[(ZoneId, MiniBar); MAX_MINIBARS]>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tree: Tree::with_capacity(64),
            viewport: Size::ZERO,
            zones: ZoneList::new(),
            next_zone_id: Cell::new(1),
            is_pinned: Cell::new(false),
            settings_open: Cell::new(false),
            settings_dirty: Cell::new(false),
            settings_draft_accent_color: RefCell::new(None),
            settings_snapshot: RefCell::new(None),
            scroll_offset_y: Cell::new(0.0),
            setting_desktop_embed: Cell::new(true),
            setting_autostart: Cell::new(false),
            setting_show_in_taskbar: Cell::new(true),
            setting_smart_layout: Cell::new(true),
            setting_portable_mode: Cell::new(false),
            desktop_sources: RefCell::new(Vec::new()),
            desktop_path_draft: RefCell::new(SmolStr::new_static("D:\\Desktop")),
            watch_paths_draft: RefCell::new(SmolStr::default()),
            expand_delay_ms: Cell::new(150),
            collapse_delay_ms: Cell::new(300),
            icon_cache_size: Cell::new(500),
            startup_high_priority: Cell::new(false),
            crash_restart_enabled: Cell::new(true),
            crash_max_retries: Cell::new(3),
            crash_window_secs: Cell::new(60),
            safe_start_after_hibernation: Cell::new(true),
            hibernate_resume_delay_ms: Cell::new(2000),
            update_check_frequency: Cell::new(UpdateCheckFrequency::Weekly),
            update_auto_download: Cell::new(true),
            settings_updater_status: RefCell::new(SettingsUpdaterStatus::Idle),
            stealth_enabled: Cell::new(true),
            stealth_status: RefCell::new(None),
            encryption_mode: Cell::new(SettingsEncryptionMode::None),
            passphrase_unlock_required: Cell::new(false),
            settings_encryption_status: RefCell::new(None),
            passphrase_entry_active: Cell::new(false),
            passphrase_entry_purpose: Cell::new(PassphraseEntryPurpose::Set),
            passphrase_draft: RefCell::new(String::new()),
            theme_base_accent: RefCell::new(None),
            active_theme_id: RefCell::new(SmolStr::new_static("dark")),
            active_theme_name: RefCell::new(SmolStr::new_static("Dark")),
            active_theme_tokens: RefCell::new(DARK_DEFAULT.clone()),
            active_theme_tauri: RefCell::new(PALETTE_DARK),
            // M6b — default to the global dark/Rounded baseline; repopulated at
            // the `apply_active_theme` choke-point on boot-restore + live swap.
            active_theme_radius_tauri: RefCell::new(RADIUS),
            active_theme_shadow_tauri: RefCell::new(SHADOW),
            active_theme_typography_tauri: RefCell::new(TYPOGRAPHY),
            available_themes: RefCell::new(Vec::new()),
            settings_theme_status: RefCell::new(None),
            zone_display_mode: Cell::new(ZoneDisplayMode::default()),
            hovered_zone: Cell::new(None),
            selected_zone: Cell::new(None),
            settings_backup_status: RefCell::new(None),
            settings_backup_entries: RefCell::new(Vec::new()),
            settings_plugin_entries: RefCell::new(Vec::new()),
            settings_plugin_status: RefCell::new(None),
            settings_keybindings_open: Cell::new(false),
            settings_keybinding_recording: RefCell::new(None),
            settings_keybinding_feedback: RefCell::new(None),
            about_open: Cell::new(false),
            debug_overlay: RefCell::new(DebugOverlayState::default()),
            zones_path: PathBuf::new(),
            dirty: Cell::new(false),
            zone_drag: Cell::new(None),
            zone_drag_origin: Cell::new(None),
            zone_resize: Cell::new(None),
            item_drag: RefCell::new(None),
            stack_tray: RefCell::new(None),
            stack_tray_drag: Cell::new(None),
            stack_bloom_anchor: Cell::new(None),
            stack_bloom_started_ms: Cell::new(0),
            stack_bloom_progress: Cell::new(1.0),
            zone_pill_anim_zone: Cell::new(None),
            zone_pill_anim_started_ms: Cell::new(0),
            zone_pill_anim_progress: Cell::new(1.0),
            zone_pill_anim_expanding: Cell::new(false),
            pill_animator: RefCell::new(Animator::new()),
            pill_pressed_zone: Cell::new(None),
            hover_scheduler: Cell::new(HoverScheduler::new()),
            item_hover: Cell::new(ItemHoverState::new()),
            zone_editor: RefCell::new(None),
            item_file_rename: RefCell::new(None),
            item_operation_status: RefCell::new(None),
            icon_picker: RefCell::new(None),
            palette_picker: RefCell::new(None),
            capsule_picker: RefCell::new(CapsulePickerState::new()),
            bulk_manager: RefCell::new(BulkManagerState::new()),
            bulk_manager_status: RefCell::new(None),
            rules_wizard: RefCell::new(RulesWizardState::new()),
            rules_wizard_rules: RefCell::new(Vec::new()),
            rules_wizard_rule_cursor: Cell::new(0),
            rules_wizard_delete_confirm: RefCell::new(None),
            rules_wizard_status: RefCell::new(None),
            timeline_panel: RefCell::new(TimelinePanelState::new()),
            snapshot_picker: RefCell::new(SnapshotPickerState::new()),
            search_bar: RefCell::new(SearchBarState::default()),
            search_status: RefCell::new(None),
            highlight_overlay: RefCell::new(HighlightOverlayState::new()),
            active_tooltip: RefCell::new(None),
            suggestor: RefCell::new(SuggestorState::new()),
            suggestor_status: RefCell::new(None),
            suggestor_dismissed: RefCell::new(HashSet::new()),
            minibars: RefCell::new(SmallVec::new()),
        }
    }

    /// Mark zones as mutated this cycle. `consume_dispatcher` reads + clears.
    pub fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    /// M1a 2026-05-29 — capture the current persisted General-section toggle
    /// values for later Cancel/Escape rollback. Shell wires this on the
    /// `OpenSettings` path so even keyboard-driven launches snapshot before
    /// the user can mutate any toggle.
    pub fn snapshot_settings(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            ghost_layer_enabled: self.setting_desktop_embed.get(),
            launch_at_startup: self.setting_autostart.get(),
            show_in_taskbar: self.setting_show_in_taskbar.get(),
            auto_group_enabled: self.setting_smart_layout.get(),
            portable_mode: self.setting_portable_mode.get(),
            expand_delay_ms: self.expand_delay_ms.get(),
            collapse_delay_ms: self.collapse_delay_ms.get(),
            icon_cache_size: self.icon_cache_size.get(),
            startup_high_priority: self.startup_high_priority.get(),
            crash_restart_enabled: self.crash_restart_enabled.get(),
            crash_max_retries: self.crash_max_retries.get(),
            crash_window_secs: self.crash_window_secs.get(),
            safe_start_after_hibernation: self.safe_start_after_hibernation.get(),
            hibernate_resume_delay_ms: self.hibernate_resume_delay_ms.get(),
        }
    }

    /// M1a 2026-05-29 — restore each General-section toggle Cell from a
    /// snapshot. Used by Cancel/Escape/Close × so cancelled edits never leak
    /// past the in-memory panel. Caller is responsible for clearing
    /// `settings_dirty` and requesting a redraw.
    pub fn restore_settings(&self, snap: &SettingsSnapshot) {
        self.setting_desktop_embed.set(snap.ghost_layer_enabled);
        self.setting_autostart.set(snap.launch_at_startup);
        self.setting_show_in_taskbar.set(snap.show_in_taskbar);
        self.setting_smart_layout.set(snap.auto_group_enabled);
        self.setting_portable_mode.set(snap.portable_mode);
        self.expand_delay_ms.set(snap.expand_delay_ms);
        self.collapse_delay_ms.set(snap.collapse_delay_ms);
        self.icon_cache_size.set(snap.icon_cache_size);
        self.startup_high_priority.set(snap.startup_high_priority);
        self.crash_restart_enabled.set(snap.crash_restart_enabled);
        self.crash_max_retries.set(snap.crash_max_retries);
        self.crash_window_secs.set(snap.crash_window_secs);
        self.safe_start_after_hibernation
            .set(snap.safe_start_after_hibernation);
        self.hibernate_resume_delay_ms
            .set(snap.hibernate_resume_delay_ms);
    }

    pub fn active_theme_palette(&self) -> PaletteTokens {
        self.active_theme_tokens.borrow().palette
    }

    /// M6a — the active theme's Tauri-parity palette. `PaletteTauri: Copy`, so
    /// the renderer binds this ONCE per paint fn and reads slots by value
    /// (no per-frame alloc, no repeated RefCell borrows — §10).
    pub fn active_theme_tauri(&self) -> PaletteTauri {
        *self.active_theme_tauri.borrow()
    }

    /// M6b — the active theme's Tauri-parity radius. `Copy`, bound once per
    /// paint fn (§10). The 17 builtins return their per-theme `RadiusTauri`;
    /// custom JSON themes return the global `RADIUS`.
    pub fn active_theme_radius_tauri(&self) -> RadiusTauri {
        *self.active_theme_radius_tauri.borrow()
    }

    /// M6b — the active theme's Tauri-parity shadow stacks. `Copy`, §10.
    pub fn active_theme_shadow_tauri(&self) -> ShadowTauri {
        *self.active_theme_shadow_tauri.borrow()
    }

    /// M6b — the active theme's Tauri-parity typography (per-theme font family).
    /// `Copy`, §10.
    pub fn active_theme_typography_tauri(&self) -> TypographyTauri {
        *self.active_theme_typography_tauri.borrow()
    }

    pub fn active_theme_radius(&self) -> RadiusTokens {
        self.active_theme_tokens.borrow().radius
    }

    pub fn active_theme_spacing(&self) -> SpacingTokens {
        self.active_theme_tokens.borrow().spacing
    }

    pub fn active_theme_shadow(&self) -> ShadowTokens {
        self.active_theme_tokens.borrow().shadow
    }

    pub fn active_theme_typography(&self) -> TypoTokens {
        self.active_theme_tokens.borrow().typo.clone()
    }

    pub fn apply_active_theme(&self, id: SmolStr, name: SmolStr, tokens: ThemeTokens) -> bool {
        let mut changed = false;
        // M6a — resolve the Tauri-parity palette FIRST, while `id` + `tokens`
        // are still borrowable (both are moved into their RefCells below). The
        // 17 builtins hit a byte-exact const; custom JSON themes derive off the
        // live tokens. This is the single choke-point both boot-restore and
        // live `SetActiveTheme` route through, so one resolve covers both.
        let tauri = crate::theme_bridge::resolve_palette_tauri(id.as_str(), &tokens.palette);
        // M6b — resolve the per-theme Tauri-parity radius/shadow/typography too,
        // while `id` is still borrowable. Builtins hit the per-theme const;
        // custom JSON themes fall back to the global baseline. Same choke-point
        // as the palette so boot-restore + live `SetActiveTheme` stay in sync.
        let radius_tauri =
            bento_nano_style::tokens::radius_tauri_for_theme(id.as_str()).unwrap_or(RADIUS);
        let shadow_tauri =
            bento_nano_style::tokens::shadow_tauri_for_theme(id.as_str()).unwrap_or(SHADOW);
        let typography_tauri = bento_nano_style::tokens::typography_tauri_for_theme(id.as_str())
            .unwrap_or(TYPOGRAPHY);
        {
            let mut current_id = self.active_theme_id.borrow_mut();
            if *current_id != id {
                *current_id = id;
                changed = true;
            }
        }
        {
            let mut current_name = self.active_theme_name.borrow_mut();
            if *current_name != name {
                *current_name = name;
                changed = true;
            }
        }
        {
            let mut current_tauri = self.active_theme_tauri.borrow_mut();
            if *current_tauri != tauri {
                *current_tauri = tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_radius_tauri.borrow_mut();
            if *current != radius_tauri {
                *current = radius_tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_shadow_tauri.borrow_mut();
            if *current != shadow_tauri {
                *current = shadow_tauri;
                changed = true;
            }
        }
        {
            let mut current = self.active_theme_typography_tauri.borrow_mut();
            if *current != typography_tauri {
                *current = typography_tauri;
                changed = true;
            }
        }
        {
            let mut current_tokens = self.active_theme_tokens.borrow_mut();
            if *current_tokens != tokens {
                *current_tokens = tokens;
                changed = true;
            }
        }
        changed
    }

    /// M6a — apply ANY of the 17 builtin themes by id, end-to-end, without
    /// going through the shell's backend loader.
    ///
    /// Sets `active_theme_id` / `active_theme_name`, the renderer `ThemeTokens`
    /// (per-theme radius/shadow/font folded in via
    /// `theme_bridge::theme_tokens_for_theme`) and the byte-exact `PaletteTauri`
    /// + per-theme Tauri radius/shadow/typography (resolved inside
    /// `apply_active_theme`).
    ///
    /// M6b — closes the former documented partial (the 15 non-registry themes
    /// no longer fall back to the matching-polarity DEFAULT verbatim): the
    /// polarity default is now only the *base* (palette/spacing/line-heights),
    /// onto which `theme_tokens_for_theme` folds the theme's real per-theme
    /// radius (sharp `order`/`flat`/`brutalism`), shadow (Angular `none` flat),
    /// and font family (`terminal`→Consolas, `editorial`→Georgia).
    ///
    /// Returns `Some(changed)` for a known builtin id, `None` for an unknown
    /// id (panic-free, §11 — caller decides whether to route to the custom
    /// JSON loader instead).
    pub fn apply_active_theme_by_id(&self, id: &str) -> Option<bool> {
        // Builtin-only entry point: the id must be one of the 17. The exact
        // `PaletteTauri` is re-resolved inside `apply_active_theme`.
        let tauri = bento_nano_style::tokens::palette_tauri_for_theme(id)?;
        // Renderer ThemeTokens: registry lookup first (dark/light have authored
        // token sets — byte-identical net); the remaining 15 start from the
        // matching-polarity default as the *base* (palette/spacing) and then
        // fold in per-theme radius/shadow/font via the Family-2 bridge.
        let base = THEMES
            .iter()
            .find(|(theme_id, _)| *theme_id == id)
            .map(|(_, tokens)| (*tokens).clone())
            .unwrap_or_else(|| {
                if tauri.is_dark {
                    DARK_DEFAULT.clone()
                } else {
                    LIGHT_DEFAULT.clone()
                }
            });
        let tokens = crate::theme_bridge::theme_tokens_for_theme(id, &base);
        let name = builtin_theme_display_name(id);
        Some(self.apply_active_theme(SmolStr::new(id), name, tokens))
    }

    pub fn set_available_themes(&self, themes: Vec<ThemeOption>) -> bool {
        let mut current = self.available_themes.borrow_mut();
        if *current == themes {
            return false;
        }
        *current = themes;
        true
    }

    pub fn set_settings_plugins(&self, plugins: Vec<SettingsPluginEntry>) -> bool {
        let mut current = self.settings_plugin_entries.borrow_mut();
        if *current == plugins {
            return false;
        }
        *current = plugins;
        true
    }

    pub fn set_zone_display_mode(&self, mode: ZoneDisplayMode) -> bool {
        let changed = self.zone_display_mode.get() != mode;
        self.zone_display_mode.set(mode);
        changed
    }

    pub fn effective_zone_display_mode(&self, zone: &Zone) -> ZoneDisplayMode {
        zone.display_mode
            .as_deref()
            .and_then(ZoneDisplayMode::parse)
            .unwrap_or_else(|| self.zone_display_mode.get())
    }

    pub fn zone_body_visible_for_mode(&self, zone: &Zone) -> bool {
        match self.effective_zone_display_mode(zone) {
            ZoneDisplayMode::Always => true,
            ZoneDisplayMode::Hover => {
                self.hovered_zone.get() == Some(zone.id)
                    || self.selected_zone.get() == Some(zone.id)
            }
            ZoneDisplayMode::Click => self.selected_zone.get() == Some(zone.id),
        }
    }

    pub fn show_tooltip_text(&self, text: SmolStr) -> bool {
        let mut active = self.active_tooltip.borrow_mut();
        match active.as_mut() {
            Some(session) if session.text == text => false,
            Some(session) => {
                session.text = text;
                true
            }
            None => {
                *active = Some(TooltipSession { text });
                true
            }
        }
    }

    pub fn hide_tooltip_text(&self) -> bool {
        self.active_tooltip.borrow_mut().take().is_some()
    }

    pub fn upsert_minibar(&self, zone_id: ZoneId, bar: MiniBar) {
        let mut minibars = self.minibars.borrow_mut();
        if let Some((_, current)) = minibars
            .iter_mut()
            .find(|(candidate_id, _)| *candidate_id == zone_id)
        {
            *current = bar;
            return;
        }
        if minibars.len() < MAX_MINIBARS {
            minibars.push((zone_id, bar));
        }
    }

    pub fn remove_minibar(&self, zone_id: ZoneId) -> bool {
        let mut minibars = self.minibars.borrow_mut();
        let before = minibars.len();
        minibars.retain(|(candidate_id, _)| *candidate_id != zone_id);
        minibars.len() != before
    }

    pub fn active_minibar(&self) -> Option<(ZoneId, MiniBar)> {
        self.minibars.borrow().first().cloned()
    }

    /// Allocate a fresh zone id (monotonic, never reuses `ZoneId::INVALID`).
    pub fn alloc_zone_id(&self) -> ZoneId {
        let id = self.next_zone_id.get();
        self.next_zone_id.set(id.wrapping_add(1).max(1));
        ZoneId(id)
    }

    /// Mount `node` as the root widget. Returns the previous root id (if any).
    pub fn mount_root(&mut self, node: WidgetNode) -> NodeId {
        let id = self.tree.create("root", node);
        let _ = self.tree.set_root(id);
        id
    }

    /// Append `child_node` as a child of `parent`. Returns the new id.
    pub fn add_child(
        &mut self,
        parent: NodeId,
        debug_name: impl Into<smol_str::SmolStr>,
        child_node: WidgetNode,
    ) -> Result<NodeId, TreeError> {
        let id = self.tree.create(debug_name, child_node);
        self.tree.append_child(parent, id)?;
        Ok(id)
    }
}

/// Per-HWND state. Holds the layout engine (cache lives here per Ruling 5)
/// and any future per-window scratch buffers. Constructed by the shell when
/// a window is created, destroyed at WM_DESTROY.
#[derive(Debug)]
pub struct WindowState {
    pub layout: LayoutEngine,
    /// `false` until `Renderer::render` has had one chance to call
    /// `storage::read_zones` against `app.zones_path`. Subsequent paints
    /// short-circuit. Failure to load (corrupt / missing) still flips this
    /// — empty zones is the recovery path (Ruling A: silent continue).
    pub loaded: Cell<bool>,
    /// Phase 2.3.1a — current device DPI for this HWND (PER_MONITOR_AWARE_V2).
    /// Updated by the shell on `WM_DPICHANGED` and seeded once after window
    /// creation via `GetDpiForWindow`. Default `96` matches the Win32 100%
    /// scale baseline, so a never-updated cache cannot accidentally produce
    /// half-size output. `Cell` (not `RefCell`) because `u32` is `Copy`.
    pub dpi: Cell<u32>,
    /// Phase 2.3.1a — cached enumeration of all attached monitors. Refreshed
    /// after window creation and again on every `WM_DPICHANGED` (because a
    /// DPI change typically coincides with a display reconfiguration). The
    /// 4-element inline capacity matches `bento_nano_platform::monitor`'s
    /// `enumerate_monitors` to keep the typical workstation case heap-free.
    /// Phase 2.4 will route zones to monitors against this cache.
    pub monitors: SmallVec<[MonitorInfo; 4]>,
    /// Wave 15 — Tier 0 #29/#31 one-shot guard. `false` until the first
    /// successful `Renderer::render` returns; the shell's WM_PAINT handler
    /// then calls `EmptyWorkingSet(GetCurrentProcess())` exactly once and
    /// flips this to `true`. Subsequent paints short-circuit so we never
    /// pay the working-set trim cost twice (re-enabling it on every paint
    /// would page-fault the next frame's hot resources back in).
    ///
    /// Reader: WM_PAINT handler in `bento-nano-shell/src/main.rs` (the same
    /// `if !first_paint_done.get()` site that issues the trim). Writer:
    /// the same site flips `set(true)` immediately after the trim returns.
    /// `Cell` (not `RefCell`) because `bool` is `Copy` and the WM_PAINT
    /// handler is single-threaded by Win32 message-pump contract.
    pub first_paint_done: Cell<bool>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            layout: LayoutEngine::default(),
            loaded: Cell::new(false),
            // 96 DPI = 100% scale (Win32 USER_DEFAULT_SCREEN_DPI). Picked over
            // 0 so any code path reading `dpi` before WM_DPICHANGED / the
            // post-create seed gets a usable scale factor instead of dividing
            // through zero in the eventual Phase 2.3.1b scaling math.
            dpi: Cell::new(96),
            // Empty until the shell calls `enumerate_monitors()` post-create.
            // Phase 2.3.1b / 2.4 callers must tolerate the empty-cache window
            // between WM_NCCREATE and the first paint.
            monitors: SmallVec::new(),
            // Wave 15 — Tier 0 #29/#31 one-shot trim guard, defaults to
            // `false` so the very first WM_PAINT triggers the EmptyWorkingSet
            // call. After the first successful paint the shell flips this
            // to `true` and never trims again (re-trimming would just
            // page-fault hot resources back in on the next frame).
            first_paint_done: Cell::new(false),
        }
    }
}

impl WindowState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run a layout pass over `app.tree` at `app.viewport`. Cached — see
    /// `LayoutEngine::layout_with_epoch` for the invalidation key.
    pub fn run_layout(&mut self, app: &AppState) -> Result<(), LayoutError> {
        self.layout.layout(&app.tree, app.viewport).map(|_| ())
    }

    /// Test-helper: construct a `WindowState` with a known monitor list
    /// pre-seeded. Production code populates `monitors` via `paint()`'s
    /// lazy-init seed (Ruling 4 / Wave 7) or the `WM_DPICHANGED` /
    /// `WM_DISPLAYCHANGE` handlers (Phase 2.4 / Ruling 1). Integration tests
    /// living in `tests/` cannot touch private fields, so this helper is
    /// the only sanctioned construction path that bypasses the empty-cache
    /// default. `#[doc(hidden)]` keeps it out of the public rustdoc surface
    /// while still being callable from cross-crate test harnesses.
    #[doc(hidden)]
    pub fn with_monitors_for_test(monitors: SmallVec<[MonitorInfo; 4]>) -> Self {
        Self {
            monitors,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use bento_nano_style::{BorderRadius, Color};
    use bento_nano_theme::{ThemeTokens, palette, radius, shadow, spacing, typo};

    use super::*;

    #[test]
    fn active_theme_exposes_non_palette_tokens_for_renderer() {
        let app = AppState::new();
        let mut tokens = ThemeTokens {
            palette: palette::DARK,
            spacing: spacing::DEFAULT,
            radius: radius::DEFAULT,
            shadow: shadow::DEFAULT,
            typo: typo::TypoTokens {
                font_family: SmolStr::new_static("Segoe UI"),
                sizes: typo::FontSizes {
                    xs: 10.0,
                    sm: 12.0,
                    md: 14.0,
                    lg: 18.0,
                    xl: 22.0,
                    xxl: 28.0,
                },
                weights: typo::FontWeights {
                    normal: 400,
                    medium: 500,
                    bold: 700,
                },
                line_heights: typo::LineHeights {
                    tight: 1.1,
                    normal: 1.4,
                    loose: 1.7,
                },
            },
        };
        tokens.radius.md = BorderRadius::all(9.0);
        tokens.radius.xl = BorderRadius::all(18.0);
        tokens.spacing.md = 11.0;
        tokens.shadow.md.offset_y = 5.0;
        tokens.shadow.md.blur = 14.0;
        tokens.shadow.md.color = Color::from_u8(0x10, 0x11, 0x12, 0x80);
        tokens.typo.font_family = SmolStr::new_static("Segoe UI Variable");
        tokens.typo.sizes.md = 15.0;

        assert!(app.apply_active_theme(
            SmolStr::new_static("test-token-theme"),
            SmolStr::new_static("Test Token Theme"),
            tokens,
        ));

        assert_eq!(app.active_theme_radius().md, BorderRadius::all(9.0));
        assert_eq!(app.active_theme_radius().xl, BorderRadius::all(18.0));
        assert_eq!(app.active_theme_spacing().md, 11.0);
        assert_eq!(app.active_theme_shadow().md.offset_y, 5.0);
        assert_eq!(app.active_theme_shadow().md.blur, 14.0);
        assert_eq!(
            app.active_theme_shadow().md.color,
            Color::from_u8(0x10, 0x11, 0x12, 0x80)
        );
        assert_eq!(
            app.active_theme_typography().font_family.as_str(),
            "Segoe UI Variable"
        );
        assert_eq!(app.active_theme_typography().sizes.md, 15.0);
    }

    #[test]
    fn fresh_appstate_active_theme_tauri_is_dark_default() {
        // Boot default must be byte-identical to PALETTE_DARK.
        let app = AppState::new();
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_DARK,
        );
    }

    #[test]
    fn apply_dark_by_id_yields_exact_palette_dark() {
        let app = AppState::new();
        // Move off dark first so the apply is observable as a change.
        assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
        assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_DARK,
        );
        assert_eq!(app.active_theme_id.borrow().as_str(), "dark");
    }

    #[test]
    fn apply_ocean_blue_by_id_yields_exact_palette_ocean_blue() {
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_OCEAN_BLUE,
        );
        assert_eq!(app.active_theme_id.borrow().as_str(), "ocean-blue");
        // ocean-blue has no authored ThemeTokens — falls back to the dark
        // default by polarity (documented partial; widgets only).
        assert_eq!(
            app.active_theme_palette().bg,
            bento_nano_theme::DARK_DEFAULT.palette.bg,
        );
    }

    #[test]
    fn apply_light_by_id_yields_exact_palette_light_and_polarity() {
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("light"), Some(true));
        let pal = app.active_theme_tauri();
        assert_eq!(pal, bento_nano_style::tokens::PALETTE_LIGHT);
        assert!(!pal.is_dark);
        // light HAS an authored ThemeTokens (registry) — uses LIGHT_DEFAULT.
        assert_eq!(
            app.active_theme_palette().bg,
            bento_nano_theme::LIGHT_DEFAULT.palette.bg,
        );
    }

    #[test]
    fn apply_all_17_builtin_ids_resolves_exact_const() {
        let app = AppState::new();
        for id in [
            "dark",
            "light",
            "midnight",
            "forest",
            "sunset",
            "frosted",
            "ocean-blue",
            "rose-gold",
            "forest-green",
            "solid",
            "order",
            "flat",
            "brutalism",
            "editorial",
            "neo",
            "terminal",
            "cyberpunk",
        ] {
            assert!(app.apply_active_theme_by_id(id).is_some(), "{id} applied");
            assert_eq!(
                Some(app.active_theme_tauri()),
                bento_nano_style::tokens::palette_tauri_for_theme(id),
                "{id} active_theme_tauri must equal its authored const",
            );
        }
    }

    #[test]
    fn m6b_apply_repopulates_per_theme_radius_shadow_typography() {
        // M6b — the choke-point repopulate fills the three new RefCells, and
        // the accessors return the per-theme const for all 17 builtins.
        let app = AppState::new();
        for id in [
            "dark", "light", "midnight", "forest", "sunset", "frosted", "ocean-blue",
            "rose-gold", "forest-green", "solid", "order", "flat", "brutalism",
            "editorial", "neo", "terminal", "cyberpunk",
        ] {
            assert!(app.apply_active_theme_by_id(id).is_some(), "{id} applied");
            assert_eq!(
                Some(app.active_theme_radius_tauri()),
                bento_nano_style::tokens::radius_tauri_for_theme(id),
                "{id} active_theme_radius_tauri must equal its authored const",
            );
            assert_eq!(
                Some(app.active_theme_shadow_tauri()),
                bento_nano_style::tokens::shadow_tauri_for_theme(id),
                "{id} active_theme_shadow_tauri must equal its authored const",
            );
            assert_eq!(
                Some(app.active_theme_typography_tauri()),
                bento_nano_style::tokens::typography_tauri_for_theme(id),
                "{id} active_theme_typography_tauri must equal its authored const",
            );
        }
    }

    #[test]
    fn m6b_order_yields_sharp_radius_via_accessor() {
        // The former documented partial is gone: applying `order` (a non-
        // registry theme) now yields its real per-theme card radius (6), not
        // the dark/light default.
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("order"), Some(true));
        assert_eq!(app.active_theme_radius_tauri().card, 6.0);
        assert_eq!(app.active_theme_radius_tauri().capsule, 8.0);
    }

    #[test]
    fn m6b_terminal_font_flows_into_family2_typo() {
        // The font-swap path reads `active_theme_typography()` (Family-2). After
        // M6b that returns Consolas for terminal (closing the partial).
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("terminal"), Some(true));
        assert_eq!(app.active_theme_typography().font_family.as_str(), "Consolas");
        assert_eq!(app.active_theme_typography_tauri().font_family, "Consolas");
        // editorial → Georgia, and its widget radius collapses to sharp 0.
        assert_eq!(app.apply_active_theme_by_id("editorial"), Some(true));
        assert_eq!(app.active_theme_typography().font_family.as_str(), "Georgia");
        assert_eq!(app.active_theme_radius().xl.top_left, 0.0);
    }

    #[test]
    fn m6b_brutalism_flattens_family2_widget_shadow() {
        // Angular `none` themes flatten the widget-chrome shadow.
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("brutalism"), Some(true));
        assert_eq!(app.active_theme_shadow().md, bento_nano_style::Shadow::NONE);
        assert!(app.active_theme_shadow_tauri().zen.is_empty());
    }

    #[test]
    fn m6b_dark_family2_stays_byte_identical() {
        // §5.3 net: dark's Family-2 tokens must equal DARK_DEFAULT exactly.
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
        assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
        assert_eq!(app.active_theme_radius(), bento_nano_theme::DARK_DEFAULT.radius);
        assert_eq!(app.active_theme_shadow(), bento_nano_theme::DARK_DEFAULT.shadow);
        assert_eq!(
            app.active_theme_typography().font_family,
            bento_nano_theme::DARK_DEFAULT.typo.font_family,
        );
    }

    #[test]
    fn apply_unknown_id_returns_none_and_leaves_theme_unchanged() {
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("shell-purple"), None);
        // Untouched — still the dark default.
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_DARK,
        );
        assert_eq!(app.active_theme_id.borrow().as_str(), "dark");
    }

    /// α5 (S2, 2026-05-24) — pin AppState invariant that `theme_base_accent`
    /// defaults to `None`. Combined with the call-site removal in
    /// `render.rs::render_frame` (the unconditional `draw_theme_base_accent`
    /// at line 470 was deleted), a fresh launch no longer paints the 4-DIP
    /// accent strip over the desktop. If a future regression resurrects that
    /// paint call AND leaves `theme_base_accent = None`, the strip falls
    /// back to `palette.accent × 0.92` — exactly the blue strip the user
    /// reported. This test is the canary for the state half of the contract;
    /// the call-site half is pinned by the comment + git history at
    /// render.rs:470.
    #[test]
    fn theme_base_accent_defaults_to_none_alpha_s2_regression_pin() {
        let app = AppState::new();
        assert!(
            app.theme_base_accent.borrow().is_none(),
            "α5 S2 regression: theme_base_accent must default to None so a \
             future re-introduced top strip paint at least falls through the \
             swatch picker rather than the palette fallback. Setting a \
             default here would re-paint the leaked top strip the moment the \
             call site comes back."
        );
    }

    #[test]
    fn zone_pill_anim_defaults_are_settled() {
        // Wave G2 — fresh AppState must report no pill morph in flight so
        // the renderer's morph branch stays dormant until hover starts one.
        let app = AppState::new();
        assert_eq!(app.zone_pill_anim_zone.get(), None);
        assert_eq!(app.zone_pill_anim_started_ms.get(), 0);
        assert!((app.zone_pill_anim_progress.get() - 1.0).abs() < f32::EPSILON);
        assert!(!app.zone_pill_anim_expanding.get());
    }

    #[test]
    fn zone_body_visibility_respects_hover_always_click() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(7), Cow::Borrowed("docs"), 10, 10, 160, 120);

        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        assert!(!app.zone_body_visible_for_mode(&zone));
        app.hovered_zone.set(Some(zone.id));
        assert!(app.zone_body_visible_for_mode(&zone));
        app.hovered_zone.set(None);
        app.selected_zone.set(Some(zone.id));
        assert!(app.zone_body_visible_for_mode(&zone));

        app.selected_zone.set(None);
        app.set_zone_display_mode(ZoneDisplayMode::Click);
        assert!(!app.zone_body_visible_for_mode(&zone));
        app.selected_zone.set(Some(zone.id));
        assert!(app.zone_body_visible_for_mode(&zone));

        app.selected_zone.set(None);
        app.set_zone_display_mode(ZoneDisplayMode::Always);
        assert!(app.zone_body_visible_for_mode(&zone));
    }

    /// M1a 2026-05-29 — `snapshot_settings`/`restore_settings` are the single
    /// round-trip surface the Settings panel's Cancel/Escape/Close × path uses
    /// to undo unsaved General-section edits. Set all 5 toggle Cells to
    /// non-default values, snapshot, scribble different values, then restore
    /// and assert every Cell is back to the snapshotted value. Also pins that
    /// `settings_dirty` is `false` on a fresh AppState (Save dims until a row
    /// is mutated — Tauri `disabled={!dirty()}`).
    #[test]
    fn settings_snapshot_restore_round_trips_general_toggles() {
        let app = AppState::new();

        assert!(
            !app.settings_dirty.get(),
            "settings_dirty must default to false so Save starts dimmed"
        );

        // Defaults are embed=on, autostart=off, taskbar=on, smart=on,
        // portable=off. Pick the inverse of each so a no-op snapshot can't
        // pass by accident.
        app.setting_desktop_embed.set(false);
        app.setting_autostart.set(true);
        app.setting_show_in_taskbar.set(false);
        app.setting_smart_layout.set(false);
        app.setting_portable_mode.set(true);
        // M1d — set the 9 Performance/Startup fields to non-default values too.
        app.expand_delay_ms.set(200);
        app.collapse_delay_ms.set(400);
        app.icon_cache_size.set(900);
        app.startup_high_priority.set(true);
        app.crash_restart_enabled.set(false);
        app.crash_max_retries.set(7);
        app.crash_window_secs.set(45);
        app.safe_start_after_hibernation.set(false);
        app.hibernate_resume_delay_ms.set(3500);

        let snap = app.snapshot_settings();
        assert_eq!(
            snap,
            SettingsSnapshot {
                ghost_layer_enabled: false,
                launch_at_startup: true,
                show_in_taskbar: false,
                auto_group_enabled: false,
                portable_mode: true,
                expand_delay_ms: 200,
                collapse_delay_ms: 400,
                icon_cache_size: 900,
                startup_high_priority: true,
                crash_restart_enabled: false,
                crash_max_retries: 7,
                crash_window_secs: 45,
                safe_start_after_hibernation: false,
                hibernate_resume_delay_ms: 3500,
            }
        );

        // Mutate every Cell away from the snapshot (simulate cancelled edits).
        app.setting_desktop_embed.set(true);
        app.setting_autostart.set(false);
        app.setting_show_in_taskbar.set(true);
        app.setting_smart_layout.set(true);
        app.setting_portable_mode.set(false);
        app.expand_delay_ms.set(50);
        app.collapse_delay_ms.set(100);
        app.icon_cache_size.set(100);
        app.startup_high_priority.set(false);
        app.crash_restart_enabled.set(true);
        app.crash_max_retries.set(1);
        app.crash_window_secs.set(5);
        app.safe_start_after_hibernation.set(true);
        app.hibernate_resume_delay_ms.set(500);

        app.restore_settings(&snap);

        assert!(!app.setting_desktop_embed.get());
        assert!(app.setting_autostart.get());
        assert!(!app.setting_show_in_taskbar.get());
        assert!(!app.setting_smart_layout.get());
        assert!(app.setting_portable_mode.get());
        // M1d — the 9 new fields round-trip through snapshot → restore.
        assert_eq!(app.expand_delay_ms.get(), 200);
        assert_eq!(app.collapse_delay_ms.get(), 400);
        assert_eq!(app.icon_cache_size.get(), 900);
        assert!(app.startup_high_priority.get());
        assert!(!app.crash_restart_enabled.get());
        assert_eq!(app.crash_max_retries.get(), 7);
        assert_eq!(app.crash_window_secs.get(), 45);
        assert!(!app.safe_start_after_hibernation.get());
        assert_eq!(app.hibernate_resume_delay_ms.get(), 3500);
    }

    /// M1a 2026-05-29 — the Cancel/Escape/Close × path stashes the snapshot in
    /// `settings_snapshot: RefCell<Option<SettingsSnapshot>>` when the panel
    /// opens and `take()`s it on restore. Pin that the container round-trips:
    /// it starts `None`, holds the stored value, and reads back `None` after a
    /// `take()` so a second restore can't replay a stale snapshot.
    #[test]
    fn settings_snapshot_cell_round_trips_through_refcell_option() {
        let app = AppState::new();
        assert!(app.settings_snapshot.borrow().is_none());

        let snap = SettingsSnapshot {
            ghost_layer_enabled: false,
            launch_at_startup: true,
            show_in_taskbar: false,
            auto_group_enabled: true,
            portable_mode: true,
            expand_delay_ms: 150,
            collapse_delay_ms: 300,
            icon_cache_size: 500,
            startup_high_priority: false,
            crash_restart_enabled: true,
            crash_max_retries: 3,
            crash_window_secs: 60,
            safe_start_after_hibernation: true,
            hibernate_resume_delay_ms: 2000,
        };
        app.settings_snapshot.borrow_mut().replace(snap);
        assert_eq!(app.settings_snapshot.borrow().as_ref(), Some(&snap));

        let taken = app.settings_snapshot.borrow_mut().take();
        assert_eq!(taken, Some(snap));
        assert!(
            app.settings_snapshot.borrow().is_none(),
            "after take() the slot must be empty so cancel can't replay a stale snapshot"
        );
    }

    /// M1d 2026-05-29 — `slider_fraction_to_value` maps a track fraction to a
    /// stepped, clamped value. Pin the endpoints + snapping for each of the 4
    /// slider ranges so a drag can never produce an off-grid / out-of-range
    /// value. (Tauri min/max/step from `SettingsPanel.tsx:601-698`.)
    #[test]
    fn m1d_slider_fraction_clamps_and_snaps_to_step() {
        // Expand delay 50..500 step 10.
        assert_eq!(
            slider_fraction_to_value(0.0, EXPAND_DELAY_MIN_MS, EXPAND_DELAY_MAX_MS, EXPAND_DELAY_STEP_MS),
            50
        );
        assert_eq!(
            slider_fraction_to_value(1.0, EXPAND_DELAY_MIN_MS, EXPAND_DELAY_MAX_MS, EXPAND_DELAY_STEP_MS),
            500
        );
        // Below 0 / above 1 saturate at the endpoints (never out of range).
        assert_eq!(
            slider_fraction_to_value(-5.0, EXPAND_DELAY_MIN_MS, EXPAND_DELAY_MAX_MS, EXPAND_DELAY_STEP_MS),
            50
        );
        assert_eq!(
            slider_fraction_to_value(9.0, EXPAND_DELAY_MIN_MS, EXPAND_DELAY_MAX_MS, EXPAND_DELAY_STEP_MS),
            500
        );
        // Midpoint snaps to the nearest 10-step. (50 + 0.5*450 = 275 → 280).
        let mid = slider_fraction_to_value(
            0.5,
            EXPAND_DELAY_MIN_MS,
            EXPAND_DELAY_MAX_MS,
            EXPAND_DELAY_STEP_MS,
        );
        assert_eq!(mid % EXPAND_DELAY_STEP_MS, 0, "value must snap to step");
        assert!((EXPAND_DELAY_MIN_MS..=EXPAND_DELAY_MAX_MS).contains(&mid));

        // Collapse delay 100..1000 step 50 — every output is a 50-multiple.
        for n in 0..=10 {
            let f = n as f32 / 10.0;
            let v = slider_fraction_to_value(
                f,
                COLLAPSE_DELAY_MIN_MS,
                COLLAPSE_DELAY_MAX_MS,
                COLLAPSE_DELAY_STEP_MS,
            );
            assert_eq!((v - COLLAPSE_DELAY_MIN_MS) % COLLAPSE_DELAY_STEP_MS, 0);
            assert!((COLLAPSE_DELAY_MIN_MS..=COLLAPSE_DELAY_MAX_MS).contains(&v));
        }

        // Icon cache 100..2000 step 100.
        assert_eq!(
            slider_fraction_to_value(0.0, ICON_CACHE_MIN, ICON_CACHE_MAX, ICON_CACHE_STEP),
            100
        );
        assert_eq!(
            slider_fraction_to_value(1.0, ICON_CACHE_MIN, ICON_CACHE_MAX, ICON_CACHE_STEP),
            2000
        );

        // Hibernate delay 500..5000 step 100.
        assert_eq!(
            slider_fraction_to_value(0.0, HIBERNATE_DELAY_MIN_MS, HIBERNATE_DELAY_MAX_MS, HIBERNATE_DELAY_STEP_MS),
            500
        );
        assert_eq!(
            slider_fraction_to_value(1.0, HIBERNATE_DELAY_MIN_MS, HIBERNATE_DELAY_MAX_MS, HIBERNATE_DELAY_STEP_MS),
            5000
        );

        // step <= 0 degrades to a plain clamp (panic-free), never out of range.
        assert_eq!(slider_fraction_to_value(0.5, 1, 10, 0), 6);
        assert_eq!(slider_fraction_to_value(-1.0, 1, 10, 0), 1);
        assert_eq!(slider_fraction_to_value(2.0, 1, 10, 0), 10);
    }

    /// M1d 2026-05-29 — the crash steppers + sliders default to in-range Tauri
    /// values and the bounds are the exact Tauri min/max. Pin the defaults so a
    /// future edit cannot silently seed an out-of-range Cell (which the panel
    /// would then clamp on first paint, hiding the drift).
    #[test]
    fn m1d_perf_startup_defaults_in_range() {
        let app = AppState::new();
        assert!(
            (EXPAND_DELAY_MIN_MS..=EXPAND_DELAY_MAX_MS).contains(&app.expand_delay_ms.get())
        );
        assert!(
            (COLLAPSE_DELAY_MIN_MS..=COLLAPSE_DELAY_MAX_MS)
                .contains(&app.collapse_delay_ms.get())
        );
        assert!((ICON_CACHE_MIN..=ICON_CACHE_MAX).contains(&app.icon_cache_size.get()));
        assert!(
            (CRASH_MAX_RETRIES_MIN..=CRASH_MAX_RETRIES_MAX)
                .contains(&app.crash_max_retries.get())
        );
        assert!(
            (CRASH_WINDOW_SECS_MIN..=CRASH_WINDOW_SECS_MAX)
                .contains(&app.crash_window_secs.get())
        );
        assert!(
            (HIBERNATE_DELAY_MIN_MS..=HIBERNATE_DELAY_MAX_MS)
                .contains(&app.hibernate_resume_delay_ms.get())
        );
        // Bounds match Tauri exactly.
        assert_eq!((CRASH_MAX_RETRIES_MIN, CRASH_MAX_RETRIES_MAX), (1, 10));
        assert_eq!((CRASH_WINDOW_SECS_MIN, CRASH_WINDOW_SECS_MAX), (5, 60));
    }

    #[test]
    fn tooltip_session_tracks_visible_payload_and_hide() {
        let app = AppState::new();

        assert!(app.active_tooltip.borrow().is_none());
        assert!(app.show_tooltip_text(SmolStr::new_static("Open settings")));
        assert_eq!(
            app.active_tooltip
                .borrow()
                .as_ref()
                .map(|session| session.text.as_str()),
            Some("Open settings")
        );

        assert!(!app.show_tooltip_text(SmolStr::new_static("Open settings")));
        assert!(app.show_tooltip_text(SmolStr::new_static("Open vault")));
        assert_eq!(
            app.active_tooltip
                .borrow()
                .as_ref()
                .map(|session| session.text.as_str()),
            Some("Open vault")
        );

        assert!(app.hide_tooltip_text());
        assert!(app.active_tooltip.borrow().is_none());
        assert!(!app.hide_tooltip_text());
    }

    #[test]
    fn minibar_sessions_upsert_replace_and_remove() {
        let app = AppState::new();
        let first = MiniBar::new("M0 0L1 1", "Docs", 8);
        let second = MiniBar::new("M0 0L1 1", "Projects", 8);

        app.upsert_minibar(ZoneId(8), first);
        assert_eq!(
            app.active_minibar()
                .as_ref()
                .map(|(_, bar)| bar.label.as_str()),
            Some("Docs")
        );

        app.upsert_minibar(ZoneId(8), second);
        assert_eq!(app.minibars.borrow().len(), 1);
        assert_eq!(
            app.active_minibar()
                .as_ref()
                .map(|(_, bar)| bar.label.as_str()),
            Some("Projects")
        );

        assert!(app.remove_minibar(ZoneId(8)));
        assert!(app.active_minibar().is_none());
        assert!(!app.remove_minibar(ZoneId(8)));
    }
}
