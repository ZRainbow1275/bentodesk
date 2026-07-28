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

mod app_init;
mod interactions;
mod motion;
mod settings;
mod settings_edit;
mod theme;
mod window;
mod zone_surface;

pub use interactions::*;
pub use motion::*;
pub use settings::*;
pub use window::*;

use settings::{is_valid_accent_hex, normalize_accent_hex_char};

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

#[cfg(test)]
mod tests;
