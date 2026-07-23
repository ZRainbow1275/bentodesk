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
    EffectTauri, PALETTE_DARK, PaletteTauri, RADIUS, RadiusTauri, SHADOW, ShadowTauri, TYPOGRAPHY,
    TypographyTauri,
};
use bento_nano_theme::{
    DARK_DEFAULT, LIGHT_DEFAULT, PaletteTokens, RadiusTokens, ShadowTokens, SpacingTokens, THEMES,
    ThemeTokens, TypoTokens,
};
use bento_nano_tree::{NodeId, Tree, TreeError};
use bento_nano_widget::WidgetNode;
use bento_nano_zone::{DEFAULT_ZONE_DISPLAY_MODE, Zone, ZoneId, ZoneItemId, ZoneList};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::{
    animator::{AnimChannel, Animator},
    business::{
        bulk_manager_panel::BulkManagerState,
        capsule_picker::CapsulePickerState,
        debug_overlay::DebugOverlayState,
        highlight_overlay::HighlightOverlayState,
        item_card::ItemHoverState,
        minibar::{MAX_MINIBARS, MiniBar},
        popover::ContextMenuSession,
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
pub const DEFAULT_EXPAND_DELAY_MS: i32 = 90;
/// 收起延迟 / Collapse Delay — `SettingsPanel.tsx:616-618`.
pub const COLLAPSE_DELAY_MIN_MS: i32 = 100;
pub const COLLAPSE_DELAY_MAX_MS: i32 = 1000;
pub const COLLAPSE_DELAY_STEP_MS: i32 = 50;
pub const DEFAULT_COLLAPSE_DELAY_MS: i32 = 200;
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

/// V21-N193 — Settings ThemeCard selection-chrome duration. Tauri applies
/// theme surface variables immediately and transitions only the card chrome
/// with `--transition-fast: 150ms ease-out`.
pub const THEME_TRANSITION_MS: u32 = 150;
/// V21-A settings dialog open scale-in duration. Tauri `.scale-in` uses
/// `animation: scaleIn 180ms ease-out forwards`.
pub const SETTINGS_OPEN_ANIMATION_MS: u32 = 180;
/// V21-A settings dialog starts at the Tauri `scaleIn` source scale.
pub const SETTINGS_OPEN_SCALE_FROM: f32 = 0.96;

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

/// M7 (2026-06-01) — which settings text field currently has keyboard focus
/// (caret), or `None`. Generalises the passphrase-only `passphrase_entry_active`
/// flag so the inline §2 桌面路径 / 监控值 inputs AND the §10 passphrase row all
/// route through one WM_CHAR/WM_KEYDOWN dispatch. The non-passphrase arms mutate
/// the `desktop_path_draft` / `watch_paths_draft` drafts directly; `Passphrase`
/// mirrors `passphrase_entry_active` for caret rendering while keeping the
/// already-wired commit-on-Enter flow (`SetEncryptionPassphrase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTextField {
    #[default]
    None,
    DesktopPath,
    WatchValues,
    AccentColor,
    Passphrase,
}

/// M7 — char cap for the 桌面路径 single-line input (Windows `MAX_PATH`-ish; an
/// inline-friendly `SmolStr` length). Counted in scalar values, not bytes.
pub const SETTINGS_DESKTOP_PATH_DRAFT_LIMIT: usize = 260;
/// M7 — char cap for the 监控值 multi-line textarea (one path per line; `\n` is
/// allowed and NOT treated as a control reject). Counted in scalar values.
pub const SETTINGS_WATCH_VALUES_DRAFT_LIMIT: usize = 1024;
/// V21-N15 — char cap for the inline Appearance accent editor (`#rrggbb`).
pub const SETTINGS_ACCENT_COLOR_DRAFT_LIMIT: usize = 7;

fn normalize_accent_hex_char(ch: char) -> Option<char> {
    if ch.is_ascii_hexdigit() {
        Some(ch.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_valid_accent_hex(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    bytes.len() == SETTINGS_ACCENT_COLOR_DRAFT_LIMIT
        && bytes[0] == b'#'
        && bytes[1..].iter().all(u8::is_ascii_hexdigit)
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
// W2 (#7 fix wave) — `Copy` was dropped when the two `SmolStr` text drafts were
// added (a heap-backed `SmolStr` is not `Copy`); `Clone` is retained and the few
// callers that previously relied on `Copy` (the two snapshot tests) clone instead.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    // Appearance is previewed live while Settings is open, but follows the
    // same Save/Cancel transaction as the other rows. Keep both values in the
    // snapshot so Cancel restores the renderer and zone visibility semantics.
    pub active_theme_id: SmolStr,
    pub zone_display_mode: ZoneDisplayMode,
    // W2 (#7 fix wave 2026-06-01) — the §2 Paths drafts are Save-gated (NOT
    // immediate), so Cancel/Escape must revert them too. They were silently
    // ignored by snapshot/restore before this fix, leaking mid-edit path/watch
    // mutations for the rest of the session (state.rs invariant §148-151).
    pub desktop_path_draft: SmolStr,
    pub watch_paths_draft: SmolStr,
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

/// Pointer-intent state for the compact stack Bloom surface.
///
/// The Tauri reference keeps the Bloom alive briefly while the cursor crosses
/// the small gaps between its capsule, petals, and focused preview. It also
/// distinguishes an incidental petal sweep from a deliberate hover before
/// opening that member's preview. Keeping those related deadlines in one
/// copyable cell prevents render, hit-test, and the frame timer from observing
/// a half-updated combination of flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StackBloomInteractionState {
    /// First blank-space sample while a Bloom is open; cancelled by re-entry.
    pub leave_started_ms: Option<u32>,
    /// Petal currently carrying the immediate active visual.
    pub active_member: Option<ZoneId>,
    /// Timestamp used by the 150 ms petal hover-intent gate.
    pub active_member_started_ms: u32,
    /// First sample outside the active petal; gives the active ring a short
    /// gap-crossing grace before it clears.
    pub active_member_leave_started_ms: Option<u32>,
    /// Prevents a consumed hover intent from reopening a preview until a fresh
    /// petal enter occurs.
    pub hover_preview_opened: bool,
    /// True only after an explicit petal click commits the focused preview.
    pub preview_sticky: bool,
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

/// Expanded PanelHeader action button kind shared by shell hit-testing and D2D paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelHeaderButtonKind {
    /// Magnifier button that opens Search.
    Search,
    /// Close button that collapses the expanded panel.
    Close,
}

/// Currently hovered expanded PanelHeader button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelHeaderButtonHover {
    pub zone_id: ZoneId,
    pub button: PanelHeaderButtonKind,
}

impl PanelHeaderButtonHover {
    pub const fn new(zone_id: ZoneId, button: PanelHeaderButtonKind) -> Self {
        Self { zone_id, button }
    }
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
    /// V21-A — raw `GetTickCount` timestamp captured by the real Settings
    /// producer. The renderer samples it for the open scale-in transform.
    pub settings_open_started_ms: Cell<u32>,
    /// M1a 2026-05-29 — `true` after the user mutates any persisted Settings
    /// row in the open panel. Save dims when `false` (matches Tauri
    /// `disabled={!dirty()}` at `SettingsPanel.tsx:799`); Save/Cancel clear it.
    pub settings_dirty: Cell<bool>,
    /// Visible Save failure kept in the sticky footer. A failed validation,
    /// vault flush, or native side effect must leave Settings open and dirty
    /// instead of silently closing as if the change had succeeded.
    pub settings_save_error: RefCell<Option<SmolStr>>,
    /// M6-UI 2026-05-29 — in-flight accent-colour draft picked in the §3
    /// Appearance accent row (Control B). `Some("#rrggbb")` after a swatch
    /// click; the renderer rings the matching swatch, and Save persists it via
    /// the `accent_color` config-vault key. `None` falls back to the persisted
    /// `theme_base_accent`. Cancel clears the draft. `RefCell` because the
    /// value is an owned `SmolStr` (not `Copy`).
    pub settings_draft_accent_color: RefCell<Option<SmolStr>>,
    /// V21-N16 - explicit request to reset the saved Appearance accent. This
    /// must stay separate from the hex draft: an empty or malformed draft is an
    /// editable field state, while this flag means Save should delete the
    /// persisted accent keys and fall back to the default blue.
    pub settings_accent_clear_requested: Cell<bool>,
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
    /// Round-2 M1 — top-section toggle: 显示在任务栏. Default off, matching
    /// the Tauri desktop-overlay default.
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
    /// each classified into a [`DesktopSourceKind`] and tagged with Tauri's
    /// `watched` source flag. The shell repopulates this on Settings-open and on
    /// the Refresh button (`RefreshDesktopSources`); the
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
    /// M7 (2026-06-01) — which settings text field currently has keyboard
    /// focus (caret), or `None`. Drives WM_CHAR/WM_KEYDOWN routing for the
    /// inline settings body. `Passphrase` mirrors `passphrase_entry_active`
    /// for caret rendering; the non-passphrase arms edit the drafts above.
    pub settings_focused_field: Cell<SettingsTextField>,
    /// M1d 2026-05-29 — Performance §5 slider: 展开延迟 / Expand Delay in ms
    /// (50..=500, step 10). Save-gated; reverted by Cancel. The 90 ms release
    /// default filters fly-over input without making the structural response
    /// feel delayed.
    pub expand_delay_ms: Cell<i32>,
    /// M1d — Performance §5 slider: 收起延迟 / Collapse Delay in ms
    /// (100..=1000, step 50). The 200 ms release default preserves a short
    /// re-entry grace while keeping the close response quick.
    pub collapse_delay_ms: Cell<i32>,
    /// M1d — Performance §5 slider: 图标缓存大小 / Icon Cache Size
    /// (100..=2000, step 100, no unit). Tauri default 500
    /// (`SettingsPanel.tsx:624`).
    pub icon_cache_size: Cell<i32>,
    /// M1d — Startup management §6 toggle: 高优先级启动 / High Priority
    /// Startup (always shown). Tauri default off (`SettingsPanel.tsx:639`).
    pub startup_high_priority: Cell<bool>,
    /// M1d — Startup management §6 toggle: 崩溃自动重启 / Crash Auto Restart
    /// (always shown). Gates the two crash steppers below. Tauri default off.
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
    /// Hovered Settings encryption mode button. This is intentionally narrow:
    /// the Settings panel only needs the Tauri `.encryption-mode-btn:hover`
    /// paint channel today, so we avoid a heap-backed generic hover map.
    pub settings_encryption_mode_hover: Cell<Option<SettingsEncryptionMode>>,
    /// Hovered Settings Appearance card/accent swatch. This stays as narrow as
    /// the inline §3 grid contract instead of adding a generic hover map.
    pub settings_appearance_hover: Cell<Option<crate::theme_picker::AppearanceHit>>,
    /// Hovered Settings header close button. Kept as a single bit so the
    /// Settings panel does not grow a heap-backed generic hover map.
    pub settings_close_hover: Cell<bool>,
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
    /// M6c — the active theme's Tauri-parity effect channel (the 4th per-theme
    /// token family). 3 builtins set one (`terminal`→scanlines,
    /// `cyberpunk`→neon, `editorial`→chromatic); the other 14 + custom JSON
    /// themes resolve to `EffectTauri::None`. Repopulated at the same
    /// `apply_active_theme` choke-point so boot-restore + live `SetActiveTheme`
    /// stay in lockstep; read on the hot path via the `Copy`-returning
    /// `active_theme_effect_tauri()` accessor. Drives the 3 M6c render
    /// primitives (scanline overlay / neon glow / chromatic title split).
    pub active_theme_effect_tauri: RefCell<EffectTauri>,
    /// V21-N193 — previous builtin ThemeCard id while Settings selection chrome
    /// fades from the old card to the new one. `None` also covers custom themes
    /// that have no inline builtin card.
    pub theme_transition_from_card: Cell<Option<u8>>,
    /// Raw `GetTickCount` timestamp captured by the live Settings producer.
    pub theme_transition_started_ms: Cell<u32>,
    /// True only while Settings ThemeCard selection chrome is transitioning.
    pub theme_transition_active: Cell<bool>,
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
    /// Expanded PanelHeader button under the pointer. This unlocks Tauri's
    /// `.panel-header__btn:hover` chrome without adding a per-button heap map.
    pub panel_header_button_hover: Cell<Option<PanelHeaderButtonHover>>,
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
    /// Card index awaiting an explicit uninstall confirmation. This mirrors the
    /// Tauri plugin card's two-step destructive action and is cleared whenever
    /// Settings closes or the registry refreshes.
    pub settings_plugin_uninstall_confirm: Cell<Option<usize>>,
    /// Suppress Settings' outside-click timer while an owned native common
    /// dialog is closing and until the mouse button that accepted/cancelled it
    /// has been released. Without this guard the picker button's screen-space
    /// click is replayed as an outside click and hides Settings before the
    /// selected result can be shown.
    pub settings_owned_dialog_release_guard: Cell<bool>,
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
    /// Whether the dragged zone's body was visible before mouse-down selection.
    /// The live drag always renders the collapsed capsule; this snapshot exists
    /// only so release logic can avoid re-expanding a panel that was selected
    /// before the drag began.
    pub zone_drag_body_visible_at_start: Cell<Option<(ZoneId, bool)>>,
    /// Selection active before the current zone drag began. A drag that started
    /// from a collapsed pill is not a click, so mouse-up restores this selection
    /// instead of leaving the dragged pill expanded.
    pub zone_drag_selected_before_start: Cell<Option<ZoneId>>,
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
    /// True while the bloom petals are running the Tauri reverse-staggered
    /// exit keyframe after pointer leave. The anchor remains set until the
    /// visible exit window completes so render and hit-region stay aligned.
    pub stack_bloom_leaving: Cell<bool>,
    /// `GetTickCount` value captured when the current stack bloom reveal
    /// started. Stored in app state so rendering and hit-testing share the
    /// same animation phase inside the selected-stack pump.
    pub stack_bloom_started_ms: Cell<u32>,
    /// Current 0..1 reveal progress for Stack wrapper bloom frames.
    pub stack_bloom_progress: Cell<f32>,
    /// Gap grace, petal hover intent, and click-sticky state for the current
    /// Bloom. The shell advances its deadlines from the existing hover frame
    /// timer; no extra timer or worker thread is required.
    pub stack_bloom_interaction: Cell<StackBloomInteractionState>,
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
    /// Visible morph at the start of the current segment. Capturing this value
    /// makes interrupted expand/collapse reversals continue from the exact
    /// painted shape instead of mirroring raw time through a non-linear curve.
    pub zone_pill_anim_from_morph: Cell<f32>,
    /// Wall-clock duration of the current segment. Full travel is 300 ms;
    /// partial reversals scale with remaining visual distance.
    pub zone_pill_anim_duration_ms: Cell<u32>,
    /// `true` when the animation is opening (pill → expanded), `false` when
    /// closing (expanded → pill). Determines which end-state is `progress=1`.
    pub zone_pill_anim_expanding: Cell<bool>,
    /// V-8 (2026-05-21) capsule pill animator for hover / press feedback.
    /// The Wave G2 `zone_pill_anim_*` fields above drive the structural
    /// rect/radius morph in `draw_zones`. `StatusDotPulse` helpers remain in
    /// `animator.rs`, but the current paint path has no status-dot consumer,
    /// so pulse state must not keep the shell repaint pump alive by itself.
    pub pill_animator: RefCell<Animator>,
    /// V-8 — zone currently registered as "pressed" (mouse-down inside a
    /// pill rect). Cleared on mouse-up regardless of release location so
    /// the press channel never lingers if the user drags off the pill.
    pub pill_pressed_zone: Cell<Option<ZoneId>>,
    /// A3 (2026-05-29) — pure hover-intent / grace-collapse scheduler. The
    /// shell feeds it `on_enter`/`on_leave` from the cursor stream and polls
    /// it once per frame; it defers expand by `expand_delay_ms`, holds the
    /// expand-lock through the single 300ms visual morph so it cannot be
    /// race-collapsed, and defers collapse by `collapse_delay_ms` so a transient leave doesn't
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
    /// Expanded Zone whose Tauri-parity inline SearchBar currently owns the
    /// shared search query. `None` keeps the global Search HWND behavior.
    pub zone_search_target: Cell<Option<ZoneId>>,
    /// `true` while the inline search field is running its bounded collapse
    /// animation. The target stays mounted until the zero-terminal frame so
    /// paint never swaps a live field for empty space in one abrupt step.
    pub zone_search_closing: Cell<bool>,
    /// Last real click/keystroke timestamp for the inline search. The existing
    /// backend poll timer uses it to dismiss an empty, abandoned field without
    /// keeping a 60 fps timer alive for the entire idle grace period.
    pub zone_search_last_interaction_ms: Cell<u32>,
    /// Scroll position of the last wheel-scrolled expanded Zone content area.
    /// Only one Zone can own the physical wheel at a time, so a compact
    /// `(ZoneId, offset)` cell is sufficient and avoids a per-frame hash map.
    /// Switching to another Zone starts that Zone at the top; reopening or
    /// changing inline search also resets the bounded content scroll.
    pub zone_content_scroll: Cell<Option<(ZoneId, f32)>>,
    /// Foreground HWND captured before inline Zone search temporarily focuses
    /// Main for keyboard input. Stored as an integer to keep Win32 out of state.
    pub zone_search_previous_foreground: Cell<isize>,
    /// Visible SearchBar status/error text.
    pub search_status: RefCell<Option<SmolStr>>,
    /// Search/Suggestor highlight preview layer painted over real selected-stack
    /// zone/item geometry in the main HWND.
    pub highlight_overlay: RefCell<HighlightOverlayState>,
    /// Active Tooltip aux-window payload. `Command::ShowTooltip` writes this
    /// before showing `WindowKind::Tooltip`; the renderer consumes it to paint
    /// the selected-stack D2D tooltip surface from the real command payload.
    pub active_tooltip: RefCell<Option<TooltipSession>>,
    /// Active app-rendered right-click menu. The shell owns window/input and
    /// command dispatch; the renderer consumes this compact presentation model
    /// so the popup keeps one opaque D2D visual path instead of OS menu chrome.
    pub active_context_menu: RefCell<Option<ContextMenuSession>>,
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
            settings_open_started_ms: Cell::new(0),
            settings_dirty: Cell::new(false),
            settings_save_error: RefCell::new(None),
            settings_draft_accent_color: RefCell::new(None),
            settings_accent_clear_requested: Cell::new(false),
            settings_snapshot: RefCell::new(None),
            scroll_offset_y: Cell::new(0.0),
            setting_desktop_embed: Cell::new(true),
            setting_autostart: Cell::new(false),
            setting_show_in_taskbar: Cell::new(false),
            setting_smart_layout: Cell::new(true),
            setting_portable_mode: Cell::new(false),
            desktop_sources: RefCell::new(Vec::new()),
            desktop_path_draft: RefCell::new(SmolStr::new_static("D:\\Desktop")),
            watch_paths_draft: RefCell::new(SmolStr::default()),
            settings_focused_field: Cell::new(SettingsTextField::None),
            expand_delay_ms: Cell::new(DEFAULT_EXPAND_DELAY_MS),
            collapse_delay_ms: Cell::new(DEFAULT_COLLAPSE_DELAY_MS),
            icon_cache_size: Cell::new(500),
            startup_high_priority: Cell::new(false),
            crash_restart_enabled: Cell::new(false),
            crash_max_retries: Cell::new(3),
            crash_window_secs: Cell::new(10),
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
            settings_encryption_mode_hover: Cell::new(None),
            settings_appearance_hover: Cell::new(None),
            settings_close_hover: Cell::new(false),
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
            // M6c — dark default has no effect; repopulated at the choke-point.
            active_theme_effect_tauri: RefCell::new(EffectTauri::None),
            theme_transition_from_card: Cell::new(None),
            theme_transition_started_ms: Cell::new(0),
            theme_transition_active: Cell::new(false),
            available_themes: RefCell::new(Vec::new()),
            settings_theme_status: RefCell::new(None),
            zone_display_mode: Cell::new(ZoneDisplayMode::default()),
            hovered_zone: Cell::new(None),
            panel_header_button_hover: Cell::new(None),
            selected_zone: Cell::new(None),
            settings_backup_status: RefCell::new(None),
            settings_backup_entries: RefCell::new(Vec::new()),
            settings_plugin_entries: RefCell::new(Vec::new()),
            settings_plugin_status: RefCell::new(None),
            settings_plugin_uninstall_confirm: Cell::new(None),
            settings_owned_dialog_release_guard: Cell::new(false),
            settings_keybindings_open: Cell::new(false),
            settings_keybinding_recording: RefCell::new(None),
            settings_keybinding_feedback: RefCell::new(None),
            about_open: Cell::new(false),
            debug_overlay: RefCell::new(DebugOverlayState::default()),
            zones_path: PathBuf::new(),
            dirty: Cell::new(false),
            zone_drag: Cell::new(None),
            zone_drag_origin: Cell::new(None),
            zone_drag_body_visible_at_start: Cell::new(None),
            zone_drag_selected_before_start: Cell::new(None),
            zone_resize: Cell::new(None),
            item_drag: RefCell::new(None),
            stack_tray: RefCell::new(None),
            stack_tray_drag: Cell::new(None),
            stack_bloom_anchor: Cell::new(None),
            stack_bloom_leaving: Cell::new(false),
            stack_bloom_started_ms: Cell::new(0),
            stack_bloom_progress: Cell::new(1.0),
            stack_bloom_interaction: Cell::new(StackBloomInteractionState::default()),
            zone_pill_anim_zone: Cell::new(None),
            zone_pill_anim_started_ms: Cell::new(0),
            zone_pill_anim_progress: Cell::new(1.0),
            zone_pill_anim_from_morph: Cell::new(0.0),
            zone_pill_anim_duration_ms: Cell::new(
                crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS,
            ),
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
            zone_search_target: Cell::new(None),
            zone_search_closing: Cell::new(false),
            zone_search_last_interaction_ms: Cell::new(0),
            zone_content_scroll: Cell::new(None),
            zone_search_previous_foreground: Cell::new(0),
            search_status: RefCell::new(None),
            highlight_overlay: RefCell::new(HighlightOverlayState::new()),
            active_tooltip: RefCell::new(None),
            active_context_menu: RefCell::new(None),
            suggestor: RefCell::new(SuggestorState::new()),
            suggestor_status: RefCell::new(None),
            suggestor_dismissed: RefCell::new(HashSet::new()),
            minibars: RefCell::new(SmallVec::new()),
        }
    }

    /// Update the hovered expanded PanelHeader button.
    ///
    /// Returns `true` only when paint-visible hover chrome changes, letting the
    /// shell avoid redundant redraws on repeated mouse moves inside the same
    /// 28-DIP button.
    pub fn set_panel_header_button_hover(&self, hover: Option<PanelHeaderButtonHover>) -> bool {
        if self.panel_header_button_hover.get() == hover {
            return false;
        }
        self.panel_header_button_hover.set(hover);
        true
    }

    pub fn is_panel_header_button_hovered(
        &self,
        zone_id: ZoneId,
        button: PanelHeaderButtonKind,
    ) -> bool {
        self.panel_header_button_hover.get() == Some(PanelHeaderButtonHover { zone_id, button })
    }

    /// Update the hovered Settings encryption mode button.
    ///
    /// Returns `true` only when the paint-visible hover fill changes, letting
    /// the shell avoid redundant redraws on repeated mouse moves inside the
    /// same mode button.
    pub fn set_settings_encryption_mode_hover(
        &self,
        hover: Option<SettingsEncryptionMode>,
    ) -> bool {
        if self.settings_encryption_mode_hover.get() == hover {
            return false;
        }
        self.settings_encryption_mode_hover.set(hover);
        true
    }

    pub fn is_settings_encryption_mode_hovered(&self, mode: SettingsEncryptionMode) -> bool {
        self.settings_encryption_mode_hover.get() == Some(mode)
    }

    /// Update the hovered Settings Appearance ThemeCard/accent swatch.
    ///
    /// Returns `true` only when the paint-visible hover chrome changes, letting
    /// the shell avoid redundant redraws on repeated mouse moves in one card.
    pub fn set_settings_appearance_hover(
        &self,
        hover: Option<crate::theme_picker::AppearanceHit>,
    ) -> bool {
        if self.settings_appearance_hover.get() == hover {
            return false;
        }
        self.settings_appearance_hover.set(hover);
        true
    }

    pub fn is_settings_appearance_card_hovered(&self, id: u8) -> bool {
        self.settings_appearance_hover.get() == Some(crate::theme_picker::AppearanceHit::Card(id))
    }

    pub fn is_settings_appearance_accent_hovered(&self, idx: u8) -> bool {
        self.settings_appearance_hover.get()
            == Some(crate::theme_picker::AppearanceHit::Accent(idx))
    }

    pub fn is_settings_appearance_accent_editor_hovered(&self) -> bool {
        self.settings_appearance_hover.get()
            == Some(crate::theme_picker::AppearanceHit::AccentEditor)
    }

    /// Update the hovered Settings header close button.
    ///
    /// Returns `true` only when the visible hover chrome changes, mirroring the
    /// narrow Settings hover channels above.
    pub fn set_settings_close_hover(&self, hover: bool) -> bool {
        if self.settings_close_hover.get() == hover {
            return false;
        }
        self.settings_close_hover.set(hover);
        true
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
            active_theme_id: self.active_theme_id.borrow().clone(),
            zone_display_mode: self.zone_display_mode.get(),
            // W2 — capture the two §2 Paths drafts under the same snapshot so
            // Cancel/Escape replays them back (they're Save-gated like the
            // toggles, not immediate).
            desktop_path_draft: self.desktop_path_draft.borrow().clone(),
            watch_paths_draft: self.watch_paths_draft.borrow().clone(),
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
        // Built-in themes can be restored entirely inside AppState. The shell
        // follows this with its loader-backed restore path so a custom JSON
        // theme is restored with the same guarantee.
        let _ = self.apply_active_theme_by_id(snap.active_theme_id.as_str());
        self.zone_display_mode.set(snap.zone_display_mode);
        // W2 — replay the two §2 Paths drafts so a mid-edit Cancel/Escape never
        // leaks the mutated path/watch values into the rest of the session.
        *self.desktop_path_draft.borrow_mut() = snap.desktop_path_draft.clone();
        *self.watch_paths_draft.borrow_mut() = snap.watch_paths_draft.clone();
    }

    /// V21-N15 — visible value for the inline Appearance accent editor. The
    /// in-flight draft wins; otherwise we show the persisted Tauri accent and
    /// finally the blue default used by the Settings preview.
    pub fn settings_accent_editor_value(&self) -> SmolStr {
        if self.settings_accent_clear_requested.get() {
            return SmolStr::new_static("#3b82f6");
        }
        self.settings_draft_accent_color
            .borrow()
            .clone()
            .or_else(|| self.theme_base_accent.borrow().clone())
            .unwrap_or_else(|| SmolStr::new_static("#3b82f6"))
    }

    /// V21-N15 — focus the inline Appearance accent editor and seed its draft
    /// from the currently displayed value so Backspace/typing edits a real
    /// field instead of a placeholder.
    pub fn focus_settings_accent_color(&self) {
        if self.settings_accent_clear_requested.replace(false) {
            *self.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#3b82f6"));
        }
        if self.settings_draft_accent_color.borrow().is_none() {
            let seed = self.settings_accent_editor_value();
            *self.settings_draft_accent_color.borrow_mut() = Some(seed);
        }
        self.settings_focused_field
            .set(SettingsTextField::AccentColor);
    }

    /// V21-N16 - visible inline reset for the Appearance accent. The action is
    /// Save-gated: the persisted vault is only changed by `SaveSettings`.
    pub fn request_settings_accent_clear(&self) {
        self.settings_accent_clear_requested.set(true);
        self.settings_draft_accent_color.borrow_mut().take();
        self.settings_focused_field.set(SettingsTextField::None);
        self.settings_dirty.set(true);
    }

    /// V21-N16 — accept an OS colour-dialog result as the in-flight Appearance
    /// accent draft. Persistence remains Save-gated by the shell's
    /// `SaveSettings` path.
    pub fn set_settings_accent_color_from_picker(&self, hex: SmolStr) {
        *self.settings_draft_accent_color.borrow_mut() = Some(hex);
        self.settings_accent_clear_requested.set(false);
        self.settings_focused_field.set(SettingsTextField::None);
        self.settings_dirty.set(true);
    }

    /// V21-N15 — validated accent draft for persistence. Partial or malformed
    /// drafts stay visible/editable but are not flushed to the config vault.
    pub fn settings_valid_accent_draft(&self) -> Option<SmolStr> {
        if self.settings_accent_clear_requested.get() {
            return None;
        }
        let draft = self.settings_draft_accent_color.borrow();
        let raw = draft.as_deref()?;
        if is_valid_accent_hex(raw) {
            Some(SmolStr::new(raw))
        } else {
            None
        }
    }

    /// M7 (2026-06-01) — append a char into the focused NON-passphrase draft
    /// (桌面路径 / 监控值 / accent hex). Returns `true` when the draft changed. Append-only
    /// (type at end); rejects control chars (but `\n` is allowed for the
    /// WatchValues textarea); caps length by SCALAR-VALUE count (CJK-safe) so a
    /// multi-byte path char counts as one. Event-driven (one allocation per
    /// keystroke) — never on the per-frame paint path (§10). The `Passphrase`
    /// field is intentionally NOT handled here: it keeps its own
    /// `passphrase_draft` + commit-on-Enter flow via
    /// `handle_settings_passphrase_char`.
    pub fn settings_focused_push_char(&self, ch: char) -> bool {
        if self.settings_focused_field.get() == SettingsTextField::AccentColor {
            return self.settings_accent_push_char(ch);
        }
        let (draft, cap, allow_newline) = match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => (
                &self.desktop_path_draft,
                SETTINGS_DESKTOP_PATH_DRAFT_LIMIT,
                false,
            ),
            SettingsTextField::WatchValues => (
                &self.watch_paths_draft,
                SETTINGS_WATCH_VALUES_DRAFT_LIMIT,
                true,
            ),
            SettingsTextField::None
            | SettingsTextField::AccentColor
            | SettingsTextField::Passphrase => {
                return false;
            }
        };
        // Reject control chars — except a literal newline for the multi-line
        // WatchValues textarea (one watch path per line).
        if ch.is_control() && !(allow_newline && ch == '\n') {
            return false;
        }
        let mut current = draft.borrow_mut();
        if current.chars().count() >= cap {
            return false;
        }
        // SmolStr is immutable; rebuild once per keystroke (event-driven, §10).
        let mut next = String::with_capacity(current.len() + ch.len_utf8());
        next.push_str(current.as_str());
        next.push(ch);
        *current = SmolStr::new(next);
        true
    }

    /// M7 — backspace the focused NON-passphrase draft (pops the LAST scalar
    /// value, CJK-safe — never a partial byte). Returns `true` when the draft
    /// changed. Append-only edit model, so the caret is always at the end.
    pub fn settings_focused_backspace(&self) -> bool {
        if self.settings_focused_field.get() == SettingsTextField::AccentColor {
            let mut current = self.settings_draft_accent_color.borrow_mut();
            let Some(raw) = current.as_ref() else {
                return false;
            };
            if raw.is_empty() {
                return false;
            }
            let mut chars = raw.chars();
            chars.next_back();
            *current = Some(SmolStr::new(chars.collect::<String>()));
            return true;
        }
        let draft = match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => &self.desktop_path_draft,
            SettingsTextField::WatchValues => &self.watch_paths_draft,
            SettingsTextField::None
            | SettingsTextField::AccentColor
            | SettingsTextField::Passphrase => {
                return false;
            }
        };
        let mut current = draft.borrow_mut();
        if current.is_empty() {
            return false;
        }
        // Drop the final scalar value (chars() yields scalars, so collecting
        // all-but-last preserves multi-byte CJK correctly).
        let mut chars = current.chars();
        chars.next_back();
        let next: String = chars.collect();
        *current = SmolStr::new(next);
        true
    }

    /// M7 — caret index for the focused draft = its scalar-value count
    /// (append-only model, so the caret always sits at the end). Returns 0 for
    /// `None`/`Passphrase` (the passphrase field renders its own masked caret).
    pub fn settings_focused_caret(&self) -> usize {
        match self.settings_focused_field.get() {
            SettingsTextField::DesktopPath => self.desktop_path_draft.borrow().chars().count(),
            SettingsTextField::WatchValues => self.watch_paths_draft.borrow().chars().count(),
            SettingsTextField::AccentColor => self.settings_accent_editor_value().chars().count(),
            SettingsTextField::None | SettingsTextField::Passphrase => 0,
        }
    }

    fn settings_accent_push_char(&self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        let mut current = self.settings_draft_accent_color.borrow_mut();
        let raw = current.as_deref().unwrap_or("");
        if raw.chars().count() >= SETTINGS_ACCENT_COLOR_DRAFT_LIMIT {
            return false;
        }
        let mut next = String::with_capacity(raw.len() + ch.len_utf8() + 1);
        next.push_str(raw);
        if raw.is_empty() {
            if ch == '#' {
                next.push('#');
            } else if let Some(hex) = normalize_accent_hex_char(ch) {
                next.push('#');
                next.push(hex);
            } else {
                return false;
            }
        } else if let Some(hex) = normalize_accent_hex_char(ch) {
            next.push(hex);
        } else {
            return false;
        }
        self.settings_accent_clear_requested.set(false);
        *current = Some(SmolStr::new(next));
        true
    }

    pub fn active_theme_palette(&self) -> PaletteTokens {
        self.active_theme_tokens.borrow().palette
    }

    /// V21-A — start the Settings dialog scale-in animation from a live shell
    /// timestamp.
    pub fn start_settings_open_animation(&self, now_ms: u32) {
        self.settings_open_started_ms.set(now_ms);
    }

    /// V21-A — normalized Settings open progress at `now_ms`.
    pub fn settings_open_animation_progress_at(&self, now_ms: u32) -> f32 {
        settings_open_animation_progress(self.settings_open_started_ms.get(), now_ms)
    }

    /// V21-A — whether the Settings open animation still needs frame pumping.
    pub fn settings_open_animation_pending_at(&self, now_ms: u32) -> bool {
        self.settings_open.get() && self.settings_open_animation_progress_at(now_ms) < 1.0
    }

    /// M6a/V21-N193 — exact active Tauri-parity palette. Tauri updates theme
    /// surface variables immediately; only Settings ThemeCard chrome animates.
    pub fn active_theme_tauri(&self) -> PaletteTauri {
        *self.active_theme_tauri.borrow()
    }

    /// Builtin ThemeCard id for the current active theme. Custom themes have no
    /// inline card and return `None`. The 17-entry scan occurs only on a theme
    /// producer, never per frame.
    pub fn active_theme_card_id(&self) -> Option<u8> {
        let active = self.active_theme_id.borrow();
        crate::theme_picker::BUILTIN_THEMES
            .iter()
            .find(|preset| preset.theme_id == active.as_str())
            .map(|preset| preset.id)
    }

    /// Selection weight for one Settings ThemeCard at `now_ms`. The previous
    /// card fades `1→0`, the active card fades `0→1`, and all other cards stay
    /// at zero. Settled cards return exactly `0` or `1`.
    pub fn theme_card_selection_progress_at(
        &self,
        card_id: u8,
        is_active: bool,
        now_ms: u32,
    ) -> f32 {
        if !self.theme_transition_active.get() {
            return if is_active { 1.0 } else { 0.0 };
        }
        let progress = theme_transition_progress(self.theme_transition_started_ms.get(), now_ms);
        if progress >= 1.0 {
            self.theme_transition_active.set(false);
            self.theme_transition_from_card.set(None);
            return if is_active { 1.0 } else { 0.0 };
        }
        let eased = theme_transition_ease(progress);
        if is_active {
            eased
        } else if self.theme_transition_from_card.get() == Some(card_id) {
            1.0 - eased
        } else {
            0.0
        }
    }

    /// Start the existing 150ms frame lifecycle for Settings selection chrome.
    /// Global theme palettes have already switched to the target. No Settings
    /// window or no card identity change means there is nothing to animate.
    ///
    /// ponytail: one previous card is enough for normal clicks; add weighted
    /// endpoints only if sub-150ms multi-click reversal is measured in practice.
    pub fn start_theme_transition_from(&self, from_card: Option<u8>, now_ms: u32) -> bool {
        let target_card = self.active_theme_card_id();
        if !self.settings_open.get() || from_card == target_card {
            self.theme_transition_from_card.set(None);
            self.theme_transition_active.set(false);
            return false;
        }
        self.theme_transition_from_card.set(from_card);
        self.theme_transition_started_ms.set(now_ms);
        self.theme_transition_active.set(true);
        true
    }

    /// Whether Settings selection chrome still needs frame pumping at `now_ms`.
    pub fn theme_transition_pending_at(&self, now_ms: u32) -> bool {
        if !self.theme_transition_active.get() {
            return false;
        }
        if !self.settings_open.get()
            || theme_transition_progress(self.theme_transition_started_ms.get(), now_ms) >= 1.0
        {
            self.theme_transition_from_card.set(None);
            self.theme_transition_active.set(false);
            return false;
        }
        true
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

    /// M6c — the active theme's Tauri-parity effect channel. `Copy`, bound once
    /// per paint fn (§10). Returns `EffectTauri::None` for the 14 non-effect
    /// builtins + custom JSON themes; the 3 effect themes return their authored
    /// scanline/neon/chromatic descriptor.
    pub fn active_theme_effect_tauri(&self) -> EffectTauri {
        *self.active_theme_effect_tauri.borrow()
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
        let typography_tauri =
            bento_nano_style::tokens::typography_tauri_for_theme(id.as_str()).unwrap_or(TYPOGRAPHY);
        // M6c — resolve the per-theme effect (scanlines/neon/chromatic) while
        // `id` is still borrowable. 3 builtins set one; everything else (incl.
        // custom JSON) falls back to `EffectTauri::None`. Family-1 only — the
        // effect does NOT fold into `ThemeTokens` (no Family-2 bridge).
        let effect_tauri = bento_nano_style::tokens::effect_tauri_for_theme(id.as_str())
            .unwrap_or(EffectTauri::None);
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
            let mut current = self.active_theme_effect_tauri.borrow_mut();
            if *current != effect_tauri {
                *current = effect_tauri;
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
    /// together with per-theme Tauri radius/shadow/typography (resolved inside
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
        if !changed {
            return false;
        }
        self.zone_display_mode.set(mode);
        // Structural ownership belongs to the mode under which it was produced.
        // In particular, `selected_zone` is the Click-mode expansion latch. If it
        // survives Click -> Always -> Hover, Hover inherits an expanded panel
        // even with the pointer away and can no longer settle back to a capsule.
        // Clear every mode-owned latch together, then let the new mode's
        // steady-state predicate become authoritative immediately.
        self.selected_zone.set(None);
        let mut scheduler = self.hover_scheduler.get();
        scheduler.reset();
        self.hover_scheduler.set(scheduler);
        self.zone_pill_anim_zone.set(None);
        self.zone_pill_anim_progress.set(1.0);
        self.zone_pill_anim_expanding.set(false);
        self.zone_pill_anim_from_morph.set(0.0);
        true
    }

    /// Current scroll offset for `zone_id`. A different Zone never inherits
    /// the previous Zone's scroll position.
    pub fn zone_content_scroll_offset(&self, zone_id: ZoneId) -> f32 {
        self.zone_content_scroll
            .get()
            .filter(|(current, _)| *current == zone_id)
            .map(|(_, offset)| offset)
            .unwrap_or(0.0)
    }

    /// Set a finite, non-negative expanded-content scroll offset. Returning
    /// `true` means paint/hit geometry must be refreshed.
    pub fn set_zone_content_scroll(&self, zone_id: ZoneId, offset: f32) -> bool {
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let next = (offset > 0.0).then_some((zone_id, offset));
        let changed = self.zone_content_scroll.get() != next;
        self.zone_content_scroll.set(next);
        changed
    }

    pub fn reset_zone_content_scroll(&self) -> bool {
        self.zone_content_scroll.replace(None).is_some()
    }

    /// Current reveal fraction for the inline Zone search field. A manually
    /// seeded target without an animator entry is treated as settled-open so
    /// tests and restored state never produce an invisible active search.
    pub fn zone_search_animation_progress_at(&self, now_ms: u32) -> f32 {
        let Some(zone_id) = self.zone_search_target.get() else {
            return 0.0;
        };
        let animator = self.pill_animator.borrow();
        if animator.contains(zone_id, AnimChannel::InlineSearch) {
            animator.sample(zone_id, AnimChannel::InlineSearch, now_ms)
        } else if self.zone_search_closing.get() {
            0.0
        } else {
            1.0
        }
    }

    pub fn effective_zone_display_mode(&self, zone: &Zone) -> ZoneDisplayMode {
        zone.display_mode
            .as_deref()
            .and_then(ZoneDisplayMode::parse)
            .unwrap_or_else(|| self.zone_display_mode.get())
    }

    pub fn zone_body_visible_for_mode(&self, zone: &Zone) -> bool {
        // An active inline search is a transient, explicit interaction surface.
        // Keep its Zone expanded independently of Hover/Click/Always until the
        // field's reverse animation settles; otherwise leaving the capsule can
        // hide a still-focused input before its idle timeout.
        if self.zone_search_target.get() == Some(zone.id) {
            return true;
        }
        match self.effective_zone_display_mode(zone) {
            ZoneDisplayMode::Always => true,
            ZoneDisplayMode::Hover => self.hover_scheduler.get().expanded_zone() == Some(zone.id),
            ZoneDisplayMode::Click => self.selected_zone.get() == Some(zone.id),
        }
    }

    /// Shared morph gate. Both directions include raw progress 0.0: the first
    /// expand frame must paint the recorded pill/start shape rather than briefly
    /// falling through to the settled expanded renderer, and the first collapse
    /// frame must retain the complete panel before easing toward the pill.
    pub fn zone_pill_morph_in_flight(&self, zone: &Zone) -> bool {
        if zone.is_stack_anchor() || self.zone_pill_anim_zone.get() != Some(zone.id) {
            return false;
        }
        let progress = self.zone_pill_anim_progress.get();
        progress < 1.0
    }

    /// Z-order (2026-06-02) — whether `zone`'s SETTLED render surface is the
    /// expanded body (panel) rather than the collapsed pill. This is the exact
    /// `pill_body_visible` rule shared by the paint side (`Renderer::draw_zones`)
    /// and the hit sides (`effective_zone_hit_rect` / `effective_zone_chrome_rect`):
    ///
    /// - A stack anchor's body is visible only when it is explicitly SELECTED (a
    ///   focused member) — never on mere hover (hover shows the bloom).
    /// - A normal zone's body follows `zone_body_visible_for_mode`.
    /// - In BOTH cases a RESIZE (armable only on an already-expanded panel) forces
    ///   the body so the resize drag keeps the panel rect.
    /// - A zone drag always uses the collapsed capsule. Tauri collapses an
    ///   expanded panel before moving it and does not drag the large body rect.
    ///
    /// SSoT so paint, hit-rect, and z-layering can never drift.
    pub fn zone_pill_body_visible(&self, zone: &Zone) -> bool {
        let resize_id = self.zone_resize.get().map(|t| t.0);
        let is_dragged = self
            .zone_drag
            .get()
            .is_some_and(|(dragged, _, _)| dragged == zone.id);
        if is_dragged {
            return Some(zone.id) == resize_id;
        }
        if zone.is_stack_anchor() {
            self.selected_zone.get() == Some(zone.id) || Some(zone.id) == resize_id
        } else {
            self.zone_body_visible_for_mode(zone) || Some(zone.id) == resize_id
        }
    }

    /// Z-order (2026-06-02) — whether `zone` belongs to the TOP draw/hit layer.
    /// A zone is on top when its body is visible (settled expanded panel) OR a
    /// pill↔panel morph is in flight for it. The expanded/morphing zones form the
    /// top layer; all collapsed pills are the bottom layer. `draw_zones` paints
    /// the bottom layer first then the top layer (so a panel occludes any pill it
    /// overlaps); the hit/hover resolvers test the top layer first (so a point
    /// inside an expanded panel resolves to the panel, never a pill behind it).
    /// Stack anchors never run the morph, so for them this collapses to the
    /// body-visible rule. SSoT shared by paint and hit so the two can't drift.
    pub fn zone_on_top(&self, zone: &Zone) -> bool {
        if self.zone_pill_body_visible(zone) {
            return true;
        }
        // Morph in flight (pill ↔ panel). Anchors don't morph.
        self.zone_pill_morph_in_flight(zone)
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

#[inline]
pub fn settings_open_animation_progress(started_ms: u32, now_ms: u32) -> f32 {
    if SETTINGS_OPEN_ANIMATION_MS == 0 {
        return 1.0;
    }
    (now_ms.wrapping_sub(started_ms) as f32 / SETTINGS_OPEN_ANIMATION_MS as f32).clamp(0.0, 1.0)
}

#[inline]
pub fn settings_open_animation_ease(t: f32) -> f32 {
    css_ease_out(t.clamp(0.0, 1.0))
}

#[inline]
pub fn settings_open_animation_scale(eased: f32) -> f32 {
    SETTINGS_OPEN_SCALE_FROM + (1.0 - SETTINGS_OPEN_SCALE_FROM) * eased.clamp(0.0, 1.0)
}

#[inline]
pub fn theme_transition_progress(started_ms: u32, now_ms: u32) -> f32 {
    if THEME_TRANSITION_MS == 0 {
        return 1.0;
    }
    (now_ms.wrapping_sub(started_ms) as f32 / THEME_TRANSITION_MS as f32).clamp(0.0, 1.0)
}

#[inline]
pub fn theme_transition_ease(t: f32) -> f32 {
    css_ease_out(t.clamp(0.0, 1.0))
}

#[inline]
fn css_ease_out(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..12 {
        let mid = (lo + hi) * 0.5;
        if cubic_bezier_axis(0.0, 0.58, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    cubic_bezier_axis(0.0, 1.0, (lo + hi) * 0.5)
}

#[inline]
fn cubic_bezier_axis(c1: f32, c2: f32, t: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * t * c1 + 3.0 * inv * t * t * c2 + t * t * t
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use bento_nano_style::{BorderRadius, Color};
    use bento_nano_theme::{ThemeTokens, palette, radius, shadow, spacing, typo};

    use super::*;

    #[test]
    fn panel_header_button_hover_tracks_visible_changes_only() {
        let app = AppState::new();
        let search = PanelHeaderButtonHover::new(ZoneId(7), PanelHeaderButtonKind::Search);
        let close = PanelHeaderButtonHover::new(ZoneId(7), PanelHeaderButtonKind::Close);

        assert_eq!(app.panel_header_button_hover.get(), None);
        assert!(!app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Search));

        assert!(app.set_panel_header_button_hover(Some(search)));
        assert!(app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Search));
        assert!(!app.set_panel_header_button_hover(Some(search)));

        assert!(app.set_panel_header_button_hover(Some(close)));
        assert!(app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Close));

        assert!(app.set_panel_header_button_hover(None));
        assert_eq!(app.panel_header_button_hover.get(), None);
        assert!(!app.set_panel_header_button_hover(None));
    }

    #[test]
    fn settings_encryption_mode_hover_tracks_visible_changes_only() {
        let app = AppState::new();

        assert_eq!(app.settings_encryption_mode_hover.get(), None);
        assert!(!app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Dpapi));

        assert!(app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Dpapi)));
        assert!(app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Dpapi));
        assert!(!app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Dpapi)));

        assert!(app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Passphrase)));
        assert!(app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Passphrase));

        assert!(app.set_settings_encryption_mode_hover(None));
        assert_eq!(app.settings_encryption_mode_hover.get(), None);
        assert!(!app.set_settings_encryption_mode_hover(None));
    }

    #[test]
    fn settings_appearance_hover_tracks_visible_changes_only() {
        let app = AppState::new();

        assert_eq!(app.settings_appearance_hover.get(), None);
        assert!(!app.is_settings_appearance_card_hovered(5));

        assert!(
            app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Card(5)))
        );
        assert!(app.is_settings_appearance_card_hovered(5));
        assert!(!app.is_settings_appearance_accent_hovered(5));
        assert!(
            !app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Card(5)))
        );

        assert!(
            app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Accent(3)))
        );
        assert!(app.is_settings_appearance_accent_hovered(3));
        assert!(!app.is_settings_appearance_card_hovered(5));

        assert!(app.set_settings_appearance_hover(None));
        assert_eq!(app.settings_appearance_hover.get(), None);
        assert!(!app.set_settings_appearance_hover(None));
    }

    #[test]
    fn settings_close_hover_tracks_visible_changes_only() {
        let app = AppState::new();

        assert!(!app.settings_close_hover.get());
        assert!(app.set_settings_close_hover(true));
        assert!(app.settings_close_hover.get());
        assert!(!app.set_settings_close_hover(true));
        assert!(app.set_settings_close_hover(false));
        assert!(!app.settings_close_hover.get());
        assert!(!app.set_settings_close_hover(false));
    }

    #[test]
    fn settings_focused_field_default_is_none() {
        let app = AppState::new();
        assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
        // None/Passphrase fields are no-ops for the non-passphrase edit ops.
        assert!(!app.settings_focused_push_char('a'));
        app.settings_focused_field
            .set(SettingsTextField::Passphrase);
        assert!(!app.settings_focused_push_char('a'));
        assert!(!app.settings_focused_backspace());
        assert_eq!(app.settings_focused_caret(), 0);
    }

    #[test]
    fn settings_focused_push_char_appends_and_caps() {
        let app = AppState::new();
        // DesktopPath — clear the seeded default, then append. Cap = 260.
        app.settings_focused_field
            .set(SettingsTextField::DesktopPath);
        *app.desktop_path_draft.borrow_mut() = SmolStr::default();
        assert!(app.settings_focused_push_char('C'));
        assert!(app.settings_focused_push_char(':'));
        assert_eq!(app.desktop_path_draft.borrow().as_str(), "C:");
        // Control chars rejected on DesktopPath (incl. newline — single-line).
        assert!(!app.settings_focused_push_char('\n'));
        assert!(!app.settings_focused_push_char('\t'));
        assert_eq!(app.desktop_path_draft.borrow().as_str(), "C:");
        // Cap: fill to the limit, then the next push is rejected.
        *app.desktop_path_draft.borrow_mut() =
            SmolStr::new("x".repeat(SETTINGS_DESKTOP_PATH_DRAFT_LIMIT));
        assert!(!app.settings_focused_push_char('y'));
        assert_eq!(
            app.desktop_path_draft.borrow().chars().count(),
            SETTINGS_DESKTOP_PATH_DRAFT_LIMIT
        );

        // WatchValues — newline IS allowed (one path per line); other controls
        // rejected. Non-ASCII (Chinese path) accepted.
        app.settings_focused_field
            .set(SettingsTextField::WatchValues);
        *app.watch_paths_draft.borrow_mut() = SmolStr::default();
        assert!(app.settings_focused_push_char('D'));
        assert!(app.settings_focused_push_char('\n'));
        assert!(app.settings_focused_push_char('桌'));
        assert!(app.settings_focused_push_char('面'));
        assert_eq!(app.watch_paths_draft.borrow().as_str(), "D\n桌面");
        assert!(!app.settings_focused_push_char('\r'));
        assert_eq!(app.watch_paths_draft.borrow().as_str(), "D\n桌面");
    }

    #[test]
    fn settings_focused_backspace_pops_last_scalar() {
        let app = AppState::new();
        app.settings_focused_field
            .set(SettingsTextField::WatchValues);
        // Mix ASCII + a multi-byte CJK scalar; backspace must pop the scalar,
        // not a partial byte.
        *app.watch_paths_draft.borrow_mut() = SmolStr::new("a桌");
        assert!(app.settings_focused_backspace());
        assert_eq!(app.watch_paths_draft.borrow().as_str(), "a");
        assert!(app.settings_focused_backspace());
        assert_eq!(app.watch_paths_draft.borrow().as_str(), "");
        // Empty draft → no-op.
        assert!(!app.settings_focused_backspace());
    }

    #[test]
    fn settings_focused_caret_equals_char_count() {
        let app = AppState::new();
        app.settings_focused_field
            .set(SettingsTextField::DesktopPath);
        *app.desktop_path_draft.borrow_mut() = SmolStr::new("C:\\桌面");
        // 5 scalar values: C : \ 桌 面 (CJK counts as ONE each).
        assert_eq!(app.settings_focused_caret(), 5);
    }

    #[test]
    fn settings_accent_editor_seeds_from_persisted_or_default() {
        let app = AppState::new();
        assert_eq!(app.settings_accent_editor_value().as_str(), "#3b82f6");
        *app.theme_base_accent.borrow_mut() = Some(SmolStr::new_static("#f97316"));
        assert_eq!(app.settings_accent_editor_value().as_str(), "#f97316");

        app.focus_settings_accent_color();
        assert_eq!(
            app.settings_focused_field.get(),
            SettingsTextField::AccentColor
        );
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#f97316")
        );
    }

    #[test]
    fn settings_accent_clear_request_falls_back_to_default_and_refocuses_as_draft() {
        let app = AppState::new();
        *app.theme_base_accent.borrow_mut() = Some(SmolStr::new_static("#f97316"));
        *app.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#abcdef"));
        app.settings_focused_field
            .set(SettingsTextField::AccentColor);
        app.settings_dirty.set(false);

        app.request_settings_accent_clear();

        assert!(app.settings_accent_clear_requested.get());
        assert!(app.settings_draft_accent_color.borrow().is_none());
        assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
        assert!(app.settings_dirty.get());
        assert_eq!(app.settings_accent_editor_value().as_str(), "#3b82f6");
        assert_eq!(app.settings_valid_accent_draft(), None);

        app.focus_settings_accent_color();
        assert!(!app.settings_accent_clear_requested.get());
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#3b82f6")
        );
        assert_eq!(
            app.settings_focused_field.get(),
            SettingsTextField::AccentColor
        );
    }

    #[test]
    fn settings_accent_picker_result_is_save_gated_draft() {
        let app = AppState::new();
        app.settings_accent_clear_requested.set(true);
        app.settings_focused_field
            .set(SettingsTextField::AccentColor);
        app.settings_dirty.set(false);

        app.set_settings_accent_color_from_picker(SmolStr::new_static("#14b8a6"));

        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#14b8a6")
        );
        assert!(!app.settings_accent_clear_requested.get());
        assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
        assert!(app.settings_dirty.get());
        assert_eq!(
            app.settings_valid_accent_draft().as_deref(),
            Some("#14b8a6")
        );
    }

    #[test]
    fn settings_accent_editor_accepts_only_partial_hex_draft() {
        let app = AppState::new();
        app.settings_focused_field
            .set(SettingsTextField::AccentColor);
        *app.settings_draft_accent_color.borrow_mut() = None;

        assert!(app.settings_focused_push_char('A'));
        assert!(app.settings_focused_push_char('b'));
        assert!(app.settings_focused_push_char('C'));
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#abc")
        );
        assert!(!app.settings_focused_push_char('g'));
        assert!(!app.settings_focused_push_char('#'));
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#abc")
        );
        assert!(app.settings_focused_push_char('d'));
        assert!(app.settings_focused_push_char('E'));
        assert!(app.settings_focused_push_char('f'));
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#abcdef")
        );
        assert!(!app.settings_focused_push_char('0'));
        assert_eq!(
            app.settings_valid_accent_draft().as_deref(),
            Some("#abcdef")
        );
    }

    #[test]
    fn settings_accent_editor_backspace_caret_and_invalid_save_filter() {
        let app = AppState::new();
        app.settings_focused_field
            .set(SettingsTextField::AccentColor);
        *app.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#ab"));

        assert_eq!(app.settings_focused_caret(), 3);
        assert_eq!(app.settings_valid_accent_draft(), None);
        assert!(app.settings_focused_backspace());
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#a")
        );
        assert!(app.settings_focused_backspace());
        assert_eq!(
            app.settings_draft_accent_color.borrow().as_deref(),
            Some("#")
        );
        assert_eq!(app.settings_valid_accent_draft(), None);
    }

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
    fn theme_transition_progress_and_ease_match_v21_n2_contract() {
        assert_eq!(THEME_TRANSITION_MS, 150);
        assert!((theme_transition_progress(1_000, 1_000) - 0.0).abs() < f32::EPSILON);
        assert!((theme_transition_progress(1_000, 1_075) - 0.5).abs() < f32::EPSILON);
        assert!((theme_transition_progress(1_000, 1_150) - 1.0).abs() < f32::EPSILON);
        assert!((theme_transition_ease(0.25) - 0.378_138).abs() < 0.001);
        assert!((theme_transition_ease(0.5) - 0.684_643).abs() < 0.001);
        assert!((theme_transition_ease(0.75) - 0.906_535).abs() < 0.001);
        assert!(theme_transition_ease(0.5) < 0.875);
    }

    #[test]
    fn settings_open_animation_matches_v21_a_scale_in_contract() {
        assert_eq!(SETTINGS_OPEN_ANIMATION_MS, 180);
        assert!((SETTINGS_OPEN_SCALE_FROM - 0.96).abs() < f32::EPSILON);
        assert!((settings_open_animation_progress(2_000, 2_000) - 0.0).abs() < f32::EPSILON);
        assert!((settings_open_animation_progress(2_000, 2_090) - 0.5).abs() < f32::EPSILON);
        assert!((settings_open_animation_progress(2_000, 2_180) - 1.0).abs() < f32::EPSILON);

        let mid_ease = settings_open_animation_ease(0.5);
        assert!((mid_ease - 0.684_643).abs() < 0.001);
        assert!(
            (settings_open_animation_scale(0.0) - SETTINGS_OPEN_SCALE_FROM).abs() < f32::EPSILON
        );
        assert!((settings_open_animation_scale(mid_ease) - 0.987_386).abs() < 0.001);
        assert!((settings_open_animation_scale(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_open_animation_pump_only_while_open_and_unsettled() {
        let app = AppState::new();
        let started = 6_000;

        app.start_settings_open_animation(started);
        assert!(!app.settings_open_animation_pending_at(started));

        app.settings_open.set(true);
        assert!(app.settings_open_animation_pending_at(started));
        assert!(app.settings_open_animation_pending_at(started + SETTINGS_OPEN_ANIMATION_MS - 1));
        assert!(!app.settings_open_animation_pending_at(started + SETTINGS_OPEN_ANIMATION_MS));
    }

    #[test]
    fn live_theme_transition_switches_palette_immediately_and_animates_cards() {
        let app = AppState::new();
        let started = 4_000;
        app.settings_open.set(true);
        let from_card = app.active_theme_card_id();
        assert_eq!(from_card, Some(0));

        assert_eq!(app.apply_active_theme_by_id("light"), Some(true));
        assert_eq!(app.active_theme_card_id(), Some(1));
        assert_eq!(
            app.active_theme_tauri(),
            bento_nano_style::tokens::PALETTE_LIGHT
        );
        assert_eq!(app.active_theme_palette(), LIGHT_DEFAULT.palette);
        assert!(app.start_theme_transition_from(from_card, started));
        assert!(app.theme_transition_pending_at(started));

        assert_eq!(app.theme_card_selection_progress_at(0, false, started), 1.0);
        assert_eq!(app.theme_card_selection_progress_at(1, true, started), 0.0);

        let mid_ms = started + THEME_TRANSITION_MS / 2;
        let old_mid = app.theme_card_selection_progress_at(0, false, mid_ms);
        let new_mid = app.theme_card_selection_progress_at(1, true, mid_ms);
        assert!(old_mid > 0.0 && old_mid < 1.0);
        assert!(new_mid > 0.0 && new_mid < 1.0);
        assert!((old_mid + new_mid - 1.0).abs() < 0.001);

        let settled_ms = started + THEME_TRANSITION_MS;
        assert_eq!(
            app.theme_card_selection_progress_at(0, false, settled_ms),
            0.0
        );
        assert_eq!(
            app.theme_card_selection_progress_at(1, true, settled_ms),
            1.0
        );
        assert!(!app.theme_transition_pending_at(settled_ms));
        assert_eq!(app.theme_transition_from_card.get(), None);

        app.settings_open.set(false);
        assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
        assert!(!app.start_theme_transition_from(Some(1), settled_ms + 1));
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
    fn m6c_apply_repopulates_per_theme_effect() {
        // M6c — the choke-point repopulate fills the new effect RefCell, and
        // the accessor returns the per-theme const for all 17 builtins (3 set
        // an effect; 14 resolve to `None`).
        use bento_nano_style::tokens::EffectTauri;
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
                Some(app.active_theme_effect_tauri()),
                bento_nano_style::tokens::effect_tauri_for_theme(id),
                "{id} active_theme_effect_tauri must equal its authored const",
            );
        }
        // The 3 effect themes resolve to their distinct variants.
        assert_eq!(app.apply_active_theme_by_id("terminal"), Some(true));
        assert!(matches!(
            app.active_theme_effect_tauri(),
            EffectTauri::Scanlines(_)
        ));
        assert_eq!(app.apply_active_theme_by_id("cyberpunk"), Some(true));
        assert!(matches!(
            app.active_theme_effect_tauri(),
            EffectTauri::Neon(_)
        ));
        assert_eq!(app.apply_active_theme_by_id("editorial"), Some(true));
        assert!(matches!(
            app.active_theme_effect_tauri(),
            EffectTauri::Chromatic(_)
        ));
        // A non-effect theme clears it back to `None`.
        assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
        assert_eq!(app.active_theme_effect_tauri(), EffectTauri::None);
    }

    #[test]
    fn m6c_unknown_id_leaves_effect_none() {
        // Applying an unknown (custom JSON) id is rejected and leaves the
        // dark-default `None` effect untouched.
        use bento_nano_style::tokens::EffectTauri;
        let app = AppState::new();
        assert_eq!(app.apply_active_theme_by_id("shell-purple"), None);
        assert_eq!(app.active_theme_effect_tauri(), EffectTauri::None);
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
        assert_eq!(
            app.active_theme_typography().font_family.as_str(),
            "Consolas"
        );
        assert_eq!(app.active_theme_typography_tauri().font_family, "Consolas");
        // editorial → Georgia, and its widget radius collapses to sharp 0.
        assert_eq!(app.apply_active_theme_by_id("editorial"), Some(true));
        assert_eq!(
            app.active_theme_typography().font_family.as_str(),
            "Georgia"
        );
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
        assert_eq!(
            app.active_theme_radius(),
            bento_nano_theme::DARK_DEFAULT.radius
        );
        assert_eq!(
            app.active_theme_shadow(),
            bento_nano_theme::DARK_DEFAULT.shadow
        );
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
    fn zone_content_scroll_is_bounded_to_its_current_zone() {
        let app = AppState::new();
        assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 0.0);

        assert!(app.set_zone_content_scroll(ZoneId(4), 86.0));
        assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 86.0);
        assert_eq!(app.zone_content_scroll_offset(ZoneId(5)), 0.0);
        assert!(!app.set_zone_content_scroll(ZoneId(4), 86.0));

        assert!(app.set_zone_content_scroll(ZoneId(5), f32::INFINITY));
        assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 0.0);
        assert_eq!(app.zone_content_scroll_offset(ZoneId(5)), 0.0);
        assert!(!app.reset_zone_content_scroll());
    }

    #[test]
    fn inline_zone_search_progress_has_stable_open_and_animated_states() {
        let app = AppState::new();
        let zone_id = ZoneId(4);
        assert_eq!(app.zone_search_animation_progress_at(100), 0.0);

        app.zone_search_target.set(Some(zone_id));
        assert_eq!(app.zone_search_animation_progress_at(100), 1.0);

        app.pill_animator.borrow_mut().start(
            zone_id,
            AnimChannel::InlineSearch,
            100,
            180,
            0.0,
            1.0,
            crate::animator::Easing::EaseOutCubic,
        );
        assert_eq!(app.zone_search_animation_progress_at(100), 0.0);
        assert!(app.zone_search_animation_progress_at(190) > 0.5);
        assert_eq!(app.zone_search_animation_progress_at(280), 1.0);

        app.zone_search_closing.set(true);
        app.pill_animator
            .borrow_mut()
            .cancel(zone_id, AnimChannel::InlineSearch);
        assert_eq!(app.zone_search_animation_progress_at(300), 0.0);
    }

    #[test]
    fn active_inline_search_holds_hover_zone_open_until_target_clears() {
        let mut app = AppState::new();
        let zone_id = ZoneId(4);
        app.zones
            .add(Zone::new(zone_id, "Search", 10, 20, 240, 180));
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        let zone = app.zones.get(zone_id).expect("zone");

        assert!(!app.zone_pill_body_visible(zone));
        app.zone_search_target.set(Some(zone_id));
        assert!(app.zone_pill_body_visible(zone));
        app.zone_search_target.set(None);
        assert!(!app.zone_pill_body_visible(zone));
    }

    #[test]
    fn zone_pill_anim_defaults_are_settled() {
        // Wave G2 — fresh AppState must report no pill morph in flight so
        // the renderer's morph branch stays dormant until hover starts one.
        let app = AppState::new();
        assert_eq!(app.zone_pill_anim_zone.get(), None);
        assert_eq!(app.zone_pill_anim_started_ms.get(), 0);
        assert!((app.zone_pill_anim_progress.get() - 1.0).abs() < f32::EPSILON);
        assert!((app.zone_pill_anim_from_morph.get() - 0.0).abs() < f32::EPSILON);
        assert_eq!(
            app.zone_pill_anim_duration_ms.get(),
            crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
        );
        assert!(!app.zone_pill_anim_expanding.get());
    }

    #[test]
    fn zone_body_visibility_respects_hover_always_click() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(7), Cow::Borrowed("docs"), 10, 10, 160, 120);

        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        assert!(!app.zone_body_visible_for_mode(&zone));
        app.hovered_zone.set(Some(zone.id));
        assert!(!app.zone_body_visible_for_mode(&zone));
        {
            let mut scheduler = app.hover_scheduler.get();
            scheduler.mark_expanded(zone.id, 100);
            app.hover_scheduler.set(scheduler);
        }
        assert!(app.zone_body_visible_for_mode(&zone));
        app.hovered_zone.set(None);
        assert!(app.zone_body_visible_for_mode(&zone));
        {
            let mut scheduler = app.hover_scheduler.get();
            scheduler.reset();
            app.hover_scheduler.set(scheduler);
        }
        assert!(!app.zone_body_visible_for_mode(&zone));
        app.selected_zone.set(Some(zone.id));
        assert!(
            !app.zone_body_visible_for_mode(&zone),
            "ordinary clicks must not expand a hover-mode Zone"
        );

        app.selected_zone.set(None);
        app.set_zone_display_mode(ZoneDisplayMode::Click);
        assert!(!app.zone_body_visible_for_mode(&zone));
        app.selected_zone.set(Some(zone.id));
        assert!(app.zone_body_visible_for_mode(&zone));

        app.selected_zone.set(None);
        app.set_zone_display_mode(ZoneDisplayMode::Always);
        assert!(app.zone_body_visible_for_mode(&zone));
    }

    #[test]
    fn changing_display_mode_cancels_stale_hover_intent_and_morph() {
        let app = AppState::new();
        let mut scheduler = app.hover_scheduler.get();
        scheduler.on_enter(ZoneId(7), 100, 150);
        app.hover_scheduler.set(scheduler);
        app.selected_zone.set(Some(ZoneId(7)));
        app.zone_pill_anim_zone.set(Some(ZoneId(7)));
        app.zone_pill_anim_progress.set(0.4);
        app.zone_pill_anim_expanding.set(true);

        assert!(app.set_zone_display_mode(ZoneDisplayMode::Click));
        assert_eq!(app.selected_zone.get(), None);
        assert!(!app.hover_scheduler.get().is_pending());
        assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
        assert_eq!(app.zone_pill_anim_zone.get(), None);
        assert_eq!(app.zone_pill_anim_progress.get(), 1.0);
        assert!(!app.zone_pill_anim_expanding.get());
    }

    #[test]
    fn click_selection_does_not_leak_into_restored_hover_mode() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(7), Cow::Borrowed("docs"), 10, 10, 160, 120);

        assert!(app.set_zone_display_mode(ZoneDisplayMode::Click));
        app.selected_zone.set(Some(zone.id));
        assert!(app.zone_body_visible_for_mode(&zone));

        assert!(app.set_zone_display_mode(ZoneDisplayMode::Always));
        assert_eq!(app.selected_zone.get(), None);
        assert!(app.zone_body_visible_for_mode(&zone));

        assert!(app.set_zone_display_mode(ZoneDisplayMode::Hover));
        assert_eq!(app.selected_zone.get(), None);
        assert!(!app.zone_body_visible_for_mode(&zone));
    }

    #[test]
    fn zone_pill_morph_in_flight_keeps_both_start_frames_on_top() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(10), Cow::Borrowed("docs"), 10, 10, 160, 120);

        app.zone_pill_anim_zone.set(Some(zone.id));
        app.zone_pill_anim_expanding.set(true);
        app.zone_pill_anim_progress.set(0.0);
        assert!(app.zone_pill_morph_in_flight(&zone));
        assert!(app.zone_on_top(&zone));

        app.zone_pill_anim_progress.set(0.25);
        assert!(app.zone_pill_morph_in_flight(&zone));
        assert!(app.zone_on_top(&zone));

        app.zone_pill_anim_expanding.set(false);
        app.zone_pill_anim_progress.set(0.0);
        assert!(app.zone_pill_morph_in_flight(&zone));
        assert!(app.zone_on_top(&zone));

        app.zone_pill_anim_progress.set(1.0);
        assert!(!app.zone_pill_morph_in_flight(&zone));
        assert!(!app.zone_on_top(&zone));
    }

    #[test]
    fn zone_drag_from_collapsed_pill_suppresses_mouse_down_selection_expand() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(8), Cow::Borrowed("docs"), 10, 10, 160, 120);

        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        assert!(!app.zone_pill_body_visible(&zone));

        app.selected_zone.set(Some(zone.id));
        app.zone_drag.set(Some((zone.id, 4, 4)));
        app.zone_drag_body_visible_at_start
            .set(Some((zone.id, false)));

        assert!(!app.zone_pill_body_visible(&zone));
        assert!(!app.zone_on_top(&zone));
    }

    #[test]
    fn zone_drag_from_expanded_body_collapses_to_capsule() {
        let app = AppState::new();
        let zone = Zone::new(ZoneId(9), Cow::Borrowed("docs"), 10, 10, 160, 120);

        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        let mut scheduler = app.hover_scheduler.get();
        scheduler.mark_expanded(zone.id, 100);
        app.hover_scheduler.set(scheduler);
        assert!(app.zone_pill_body_visible(&zone));

        app.zone_drag.set(Some((zone.id, 4, 4)));
        app.zone_drag_body_visible_at_start
            .set(Some((zone.id, true)));

        assert!(!app.zone_pill_body_visible(&zone));
        assert!(!app.zone_on_top(&zone));
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
        assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
        app.zone_display_mode.set(ZoneDisplayMode::Click);
        // W2 (#7 fix wave) — set the two §2 Paths drafts to non-default values
        // so the snapshot/restore round-trip is exercised for them too.
        *app.desktop_path_draft.borrow_mut() = SmolStr::new("E:\\Custom\\Desktop");
        *app.watch_paths_draft.borrow_mut() = SmolStr::new("E:\\Watch\\A\nE:\\Watch\\B");

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
                active_theme_id: SmolStr::new_static("ocean-blue"),
                zone_display_mode: ZoneDisplayMode::Click,
                desktop_path_draft: SmolStr::new("E:\\Custom\\Desktop"),
                watch_paths_draft: SmolStr::new("E:\\Watch\\A\nE:\\Watch\\B"),
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
        assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
        app.zone_display_mode.set(ZoneDisplayMode::Always);
        *app.desktop_path_draft.borrow_mut() = SmolStr::new("Z:\\scribbled");
        *app.watch_paths_draft.borrow_mut() = SmolStr::new("Z:\\scribbled\nZ:\\again");

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
        assert_eq!(app.active_theme_id.borrow().as_str(), "ocean-blue");
        assert_eq!(app.zone_display_mode.get(), ZoneDisplayMode::Click);
        // W2 — the two §2 Paths drafts round-trip through snapshot → restore.
        assert_eq!(
            app.desktop_path_draft.borrow().as_str(),
            "E:\\Custom\\Desktop"
        );
        assert_eq!(
            app.watch_paths_draft.borrow().as_str(),
            "E:\\Watch\\A\nE:\\Watch\\B"
        );
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
            expand_delay_ms: DEFAULT_EXPAND_DELAY_MS,
            collapse_delay_ms: DEFAULT_COLLAPSE_DELAY_MS,
            icon_cache_size: 500,
            startup_high_priority: false,
            crash_restart_enabled: true,
            crash_max_retries: 3,
            crash_window_secs: 60,
            safe_start_after_hibernation: true,
            hibernate_resume_delay_ms: 2000,
            active_theme_id: SmolStr::new_static("dark"),
            zone_display_mode: ZoneDisplayMode::Hover,
            desktop_path_draft: SmolStr::new("D:\\Desktop"),
            watch_paths_draft: SmolStr::default(),
        };
        // W2 — `SettingsSnapshot` is no longer `Copy` (it carries two `SmolStr`
        // drafts), so clone into the slot and compare against a clone.
        app.settings_snapshot.borrow_mut().replace(snap.clone());
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
            slider_fraction_to_value(
                0.0,
                EXPAND_DELAY_MIN_MS,
                EXPAND_DELAY_MAX_MS,
                EXPAND_DELAY_STEP_MS
            ),
            50
        );
        assert_eq!(
            slider_fraction_to_value(
                1.0,
                EXPAND_DELAY_MIN_MS,
                EXPAND_DELAY_MAX_MS,
                EXPAND_DELAY_STEP_MS
            ),
            500
        );
        // Below 0 / above 1 saturate at the endpoints (never out of range).
        assert_eq!(
            slider_fraction_to_value(
                -5.0,
                EXPAND_DELAY_MIN_MS,
                EXPAND_DELAY_MAX_MS,
                EXPAND_DELAY_STEP_MS
            ),
            50
        );
        assert_eq!(
            slider_fraction_to_value(
                9.0,
                EXPAND_DELAY_MIN_MS,
                EXPAND_DELAY_MAX_MS,
                EXPAND_DELAY_STEP_MS
            ),
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
            slider_fraction_to_value(
                0.0,
                HIBERNATE_DELAY_MIN_MS,
                HIBERNATE_DELAY_MAX_MS,
                HIBERNATE_DELAY_STEP_MS
            ),
            500
        );
        assert_eq!(
            slider_fraction_to_value(
                1.0,
                HIBERNATE_DELAY_MIN_MS,
                HIBERNATE_DELAY_MAX_MS,
                HIBERNATE_DELAY_STEP_MS
            ),
            5000
        );

        // step <= 0 degrades to a plain clamp (panic-free), never out of range.
        assert_eq!(slider_fraction_to_value(0.5, 1, 10, 0), 6);
        assert_eq!(slider_fraction_to_value(-1.0, 1, 10, 0), 1);
        assert_eq!(slider_fraction_to_value(2.0, 1, 10, 0), 10);
    }

    /// Performance/startup controls seed valid values. Pin the release motion
    /// defaults as well as their ranges so first-run response cannot silently
    /// drift back to the slower reference cadence.
    #[test]
    fn m1d_perf_startup_defaults_in_range() {
        let app = AppState::new();
        assert_eq!(DEFAULT_EXPAND_DELAY_MS, 90);
        assert_eq!(DEFAULT_COLLAPSE_DELAY_MS, 200);
        assert!(
            DEFAULT_EXPAND_DELAY_MS as u32 + crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
                <= 400
        );
        assert_eq!(app.expand_delay_ms.get(), DEFAULT_EXPAND_DELAY_MS);
        assert_eq!(app.collapse_delay_ms.get(), DEFAULT_COLLAPSE_DELAY_MS);
        assert!((EXPAND_DELAY_MIN_MS..=EXPAND_DELAY_MAX_MS).contains(&app.expand_delay_ms.get()));
        assert!(
            (COLLAPSE_DELAY_MIN_MS..=COLLAPSE_DELAY_MAX_MS).contains(&app.collapse_delay_ms.get())
        );
        assert!((ICON_CACHE_MIN..=ICON_CACHE_MAX).contains(&app.icon_cache_size.get()));
        assert!(
            (CRASH_MAX_RETRIES_MIN..=CRASH_MAX_RETRIES_MAX).contains(&app.crash_max_retries.get())
        );
        assert!(
            (CRASH_WINDOW_SECS_MIN..=CRASH_WINDOW_SECS_MAX).contains(&app.crash_window_secs.get())
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
