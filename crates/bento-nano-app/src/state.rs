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

use bento_nano_backend::{rules::Rule, updater::UpdateCheckFrequency};
use bento_nano_layout::{LayoutEngine, LayoutError};
use bento_nano_platform::MonitorInfo;
use bento_nano_style::Size;
use bento_nano_theme::{
    DARK_DEFAULT, PaletteTokens, RadiusTokens, ShadowTokens, SpacingTokens, ThemeTokens, TypoTokens,
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
        minibar::{MAX_MINIBARS, MiniBar},
        rules_wizard::RulesWizardState,
        search_bar::SearchBarState,
        smart_group_suggestor::SuggestorState,
        stack_tray::{StackTrayDragState, StackTrayState},
        timeline::{TimelinePanelState, snapshot_picker::SnapshotPickerState},
    },
    dispatcher::PaletteTarget,
};

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
    /// Round-2 M1 — top-section toggle: 智能自动布局. Default on.
    pub setting_smart_layout: Cell<bool>,
    /// Round-2 M1 — top-section toggle: 使用模式 (速度模式). Default off.
    pub setting_speed_mode: Cell<bool>,
    /// Round-2 M2 — 桌面源 toggle: 海桌面 (the user's personal Desktop).
    /// Default on. Real backend wiring is M3 (`DesktopSourceProbe`).
    pub source_primary_enabled: Cell<bool>,
    /// Round-2 M2 — 桌面源 toggle: 公共桌面 (`C:\Users\Public\Desktop`).
    /// Default off.
    pub source_public_enabled: Cell<bool>,
    /// Round-2 M2 — 桌面路径 draft string editing the user's primary desktop
    /// path. Wired to a single-line text input row. Persists on Save (M4).
    pub desktop_path_draft: RefCell<SmolStr>,
    /// Round-2 M2 — 监控值 draft multi-line buffer for the watch-paths
    /// textarea. One path per line. Persists on Save (M4).
    pub watch_paths_draft: RefCell<SmolStr>,
    /// Round-2 M3 — 高级洗脑启动 toggle. Default off.
    pub setting_advanced_startup: Cell<bool>,
    /// Round-2 M3 — 磁吸切换提示 toggle. Default on.
    pub setting_magnet_switch_hint: Cell<bool>,
    /// Round-2 M3 — 最大磁吸次数 (number, clamped 1..=10). Default 3.
    pub setting_max_magnet_count: Cell<i32>,
    /// Round-2 M3 — 磁吸时间 in seconds (number, clamped 1..=30). Default 10.
    pub setting_magnet_duration_s: Cell<i32>,
    /// Round-2 M3 — 快捷区分布段 toggle. Default on.
    pub setting_zone_layout_section: Cell<bool>,
    /// Round-2 M3 — 致敬时长 in ms, driven by a slider (500..=5000). Default 2000.
    pub setting_bar_count_display_ms: Cell<i32>,
    /// Round-2 M3 — 重叠版本 (architecture version label / draft).
    pub overlay_version_draft: RefCell<SmolStr>,
    /// Round-2 M3 — 装备状态 toggle. Default true (already-on indicator).
    pub equipment_state_enabled: Cell<bool>,
    /// Round-2 M3 — 磁吸状态 toggle. Default true.
    pub magnet_state_enabled: Cell<bool>,
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
    /// `true` when the native selected-stack plugin lifecycle modal is open
    /// above Settings. Rows are loaded from the real plugin registry and
    /// mutations route through backend plugin toggle/uninstall services.
    pub settings_plugins_open: Cell<bool>,
    /// Real installed plugin rows discovered from `<state_dir>/plugins`.
    pub settings_plugin_entries: RefCell<Vec<SettingsPluginEntry>>,
    /// Visible status for plugin list/install/toggle/uninstall actions.
    pub settings_plugin_status: RefCell<Option<SettingsBackupStatus>>,
    /// `true` when the keybindings modal is open above the Settings overlay.
    /// The modal is a native selected-stack D2D surface, not the Tauri
    /// KeybindingsSection webview.
    pub settings_keybindings_open: Cell<bool>,
    /// Wave J1b — `true` when the Tauri 1.2.4 swatch popup hangs below the
    /// Row 5 active-theme chip. The picker is a sibling D2D popup painted by
    /// `crate::theme_picker::paint_into`; it lives inside the Settings HWND
    /// (no new modal / HWND) and stays open across thumbnail clicks until
    /// Save / Outside / Reset closes it. `Cell` because the dispatcher and
    /// hit-tester only borrow `&AppState` and the bool is `Copy`.
    pub theme_picker_open: Cell<bool>,
    /// Wave J1b — currently highlighted swatch index inside the picker
    /// (`0..PRESET_COUNT`). The renderer paints the check-mark disc on this
    /// preset; the hit-tester updates it on every thumbnail click so the
    /// popup keeps a visible selection while the underlying backend theme
    /// resolves asynchronously. Default `0` (Default preset).
    pub theme_picker_selected: Cell<u8>,
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
            scroll_offset_y: Cell::new(0.0),
            setting_desktop_embed: Cell::new(true),
            setting_autostart: Cell::new(false),
            setting_show_in_taskbar: Cell::new(true),
            setting_smart_layout: Cell::new(true),
            setting_speed_mode: Cell::new(false),
            source_primary_enabled: Cell::new(true),
            source_public_enabled: Cell::new(false),
            desktop_path_draft: RefCell::new(SmolStr::new_static("D:\\Desktop")),
            watch_paths_draft: RefCell::new(SmolStr::default()),
            setting_advanced_startup: Cell::new(false),
            setting_magnet_switch_hint: Cell::new(true),
            setting_max_magnet_count: Cell::new(3),
            setting_magnet_duration_s: Cell::new(10),
            setting_zone_layout_section: Cell::new(true),
            setting_bar_count_display_ms: Cell::new(2000),
            overlay_version_draft: RefCell::new(SmolStr::new_static("1.1")),
            equipment_state_enabled: Cell::new(true),
            magnet_state_enabled: Cell::new(true),
            update_check_frequency: Cell::new(UpdateCheckFrequency::Weekly),
            update_auto_download: Cell::new(true),
            settings_updater_status: RefCell::new(SettingsUpdaterStatus::Idle),
            stealth_enabled: Cell::new(true),
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
            available_themes: RefCell::new(Vec::new()),
            settings_theme_status: RefCell::new(None),
            zone_display_mode: Cell::new(ZoneDisplayMode::default()),
            hovered_zone: Cell::new(None),
            selected_zone: Cell::new(None),
            settings_backup_status: RefCell::new(None),
            settings_backup_entries: RefCell::new(Vec::new()),
            settings_plugins_open: Cell::new(false),
            settings_plugin_entries: RefCell::new(Vec::new()),
            settings_plugin_status: RefCell::new(None),
            settings_keybindings_open: Cell::new(false),
            theme_picker_open: Cell::new(false),
            theme_picker_selected: Cell::new(0),
            settings_keybinding_recording: RefCell::new(None),
            settings_keybinding_feedback: RefCell::new(None),
            about_open: Cell::new(false),
            debug_overlay: RefCell::new(DebugOverlayState::default()),
            zones_path: PathBuf::new(),
            dirty: Cell::new(false),
            zone_drag: Cell::new(None),
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

    pub fn active_theme_palette(&self) -> PaletteTokens {
        self.active_theme_tokens.borrow().palette
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
            let mut current_tokens = self.active_theme_tokens.borrow_mut();
            if *current_tokens != tokens {
                *current_tokens = tokens;
                changed = true;
            }
        }
        changed
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
