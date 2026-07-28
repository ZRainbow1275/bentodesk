use super::*;

// ── M1d 2026-05-29 — Performance §5 + Startup management §6 bounds ──────
// Min/max/step lifted 1:1 from Tauri `SettingsPanel.tsx:601-698`. Stored as
// `pub const` so geometry (`settings_panel.rs`), the shell dispatch
// (`main.rs`), and unit tests share one source of truth and never drift.

/// 展开延迟 / Expand Delay — `SettingsPanel.tsx:607-609`.
pub const EXPAND_DELAY_MIN_MS: i32 = 50;
pub const EXPAND_DELAY_MAX_MS: i32 = 500;
pub const EXPAND_DELAY_STEP_MS: i32 = 10;
pub const DEFAULT_EXPAND_DELAY_MS: i32 = 60;
/// 收起延迟 / Collapse Delay — `SettingsPanel.tsx:616-618`.
pub const COLLAPSE_DELAY_MIN_MS: i32 = 100;
pub const COLLAPSE_DELAY_MAX_MS: i32 = 1000;
pub const COLLAPSE_DELAY_STEP_MS: i32 = 50;
pub const DEFAULT_COLLAPSE_DELAY_MS: i32 = 150;
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
/// V21-A settings dialog open scale-in duration, calibrated from the Tauri
/// `scaleIn` reference to the shared fast auxiliary-surface cadence.
pub const SETTINGS_OPEN_ANIMATION_MS: u32 = 160;
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

pub(super) fn normalize_accent_hex_char(ch: char) -> Option<char> {
    if ch.is_ascii_hexdigit() {
        Some(ch.to_ascii_lowercase())
    } else {
        None
    }
}

pub(super) fn is_valid_accent_hex(raw: &str) -> bool {
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
