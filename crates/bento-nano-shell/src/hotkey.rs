//! In-process hotkey routing — Phase 2.2 / Ruling 1.
//!
//! Compile-time table of `(vk, mods) → HotkeyCommand` mappings consumed by
//! `wnd_proc`'s `WM_KEYDOWN`/`WM_SYSKEYDOWN` arm. Distinct from
//! `RegisterHotKey` (a global system hotkey, scope creep deferred to a
//! later phase): this routes only keystrokes Windows already delivered to
//! our own HWND, so it is allocation-free, lock-free, and observable
//! exclusively when the window has keyboard focus.
//!
//! Spec lock:
//!   §10 — no `HashMap` / no heap allocation; `&'static` slice + linear scan.
//!   §11 — no `unwrap` / `panic`; `lookup` is a total function returning
//!         `Option<HotkeyCommand>`.
//!   §15 — module ≤ 80 LOC including bindings.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_ESCAPE, VK_F4, VK_MENU, VK_N, VK_OEM_COMMA, VK_Q, VK_S, VK_SHIFT,
    VK_SPACE, VK_T, VK_TAB, VK_Z,
};

const VK_K_KEY: u32 = 0x4B;
const VK_D_KEY: u32 = 0x44;
const VK_H_KEY: u32 = 0x48;
const VK_L_KEY: u32 = 0x4C;
const VK_M_KEY: u32 = 0x4D;
const VK_O_KEY: u32 = 0x4F;
const VK_R_KEY: u32 = 0x52;
const VK_OEM_4_KEY: u32 = 0xDB;
const VK_OEM_6_KEY: u32 = 0xDD;

/// Modifier flags read from the live keyboard state. Constructed via
/// [`ModFlags::from_keystate`] on every `WM_KEYDOWN` so we always reflect
/// the OS view of Ctrl/Shift/Alt rather than tracking it ourselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModFlags {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl ModFlags {
    /// All-false. `const` so it appears in `DEFAULT_BINDINGS` literals.
    pub const fn none() -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Snapshot the current keyboard state. `GetKeyState` returns `i16`
    /// where the high bit set means the key is down right now (low bit is
    /// the toggle state, irrelevant for modifiers).
    pub fn from_keystate() -> Self {
        // SAFETY: GetKeyState reads thread-message-queue state; canonical
        // call with no aliasing concerns.
        let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) };
        let shift = unsafe { GetKeyState(VK_SHIFT as i32) };
        let alt = unsafe { GetKeyState(VK_MENU as i32) };
        Self {
            ctrl: (ctrl as u16) & 0x8000 != 0,
            shift: (shift as u16) & 0x8000 != 0,
            alt: (alt as u16) & 0x8000 != 0,
        }
    }
}

/// Application-level intent dispatched after a hotkey lookup hit. The shell
/// translates each variant into the matching `bento_nano_app::Command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyCommand {
    /// ESC: settings open → close panel; otherwise hide window.
    Escape,
    /// Control+Space: show/hide the selected-stack main surface.
    ToggleMain,
    /// Control+Shift+N: spawn a new zone via `Command::CreateZone`.
    CreateZone,
    /// Control+Shift+D: duplicate the selected zone.
    DuplicateZone,
    /// Control+Shift+L: toggle the selected zone's locked flag.
    ToggleZoneLock,
    /// Control+Shift+H: hide visible zones or show all hidden zones.
    ToggleAllZones,
    /// Control+Shift+O: run the backend-backed Desktop auto-organizer.
    AutoOrganize,
    /// Control+Shift+R: reflow visible zones with grid layout.
    ReflowLayout,
    /// Control+Shift+M: open the selected-stack BulkManager panel.
    OpenBulkManager,
    /// Control+]: focus the next visible zone.
    FocusNextZone,
    /// Control+[: focus the previous visible zone.
    FocusPreviousZone,
    /// Ctrl+,: open or close the settings overlay.
    ToggleSettings,
    /// Ctrl+K: open the selected-stack SearchBar panel.
    OpenSearch,
    /// Ctrl+Q: shut the message loop down via `Command::QuitApp`.
    QuitApp,
    /// Ctrl+T: open the selected-stack timeline panel.
    OpenTimeline,
    /// Ctrl+Shift+S: open the selected-stack layout SnapshotPicker.
    OpenSnapshotPicker,
    /// Ctrl+Z: restore the previous checkpoint.
    UndoCheckpoint,
    /// Ctrl+Shift+Z: restore the next checkpoint.
    RedoCheckpoint,
}

/// One entry of the compile-time hotkey table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub vk: u32,
    pub mods: ModFlags,
    pub command: HotkeyCommand,
}

/// Why a candidate runtime binding cannot replace the current table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingValidationError {
    UnsupportedActionOrChord,
    ChordAlreadyAssigned,
}

/// Stable settings action names used by the 1.x keybindings store and the
/// selected-stack config-vault keys (`keybinding.<action>`).
pub const ACTION_APP_TOGGLE: &str = "app.toggle";
pub const ACTION_CREATE_ZONE: &str = "zone.new";
pub const ACTION_DUPLICATE_ZONE: &str = "zone.duplicate";
pub const ACTION_ZONE_LOCK_TOGGLE: &str = "zone.lock-toggle";
pub const ACTION_ZONE_HIDE_ALL: &str = "zone.hide-all";
pub const ACTION_LAYOUT_AUTO_ORGANIZE: &str = "layout.auto-organize";
pub const ACTION_LAYOUT_REFLOW: &str = "layout.reflow";
pub const ACTION_BULK_OPEN_MANAGER: &str = "bulk.open-manager";
pub const ACTION_ZONE_FOCUS_NEXT: &str = "zone.focus.next";
pub const ACTION_ZONE_FOCUS_PREV: &str = "zone.focus.prev";
pub const ACTION_TOGGLE_SETTINGS: &str = "settings.open";
pub const ACTION_OPEN_SEARCH: &str = "search.open";
pub const ACTION_QUIT_APP: &str = "app.quit";
pub const ACTION_OPEN_TIMELINE: &str = "timeline.open";
pub const ACTION_OPEN_SNAPSHOT_PICKER: &str = "snapshot.open";
pub const ACTION_UNDO_CHECKPOINT: &str = "timeline.undo";
pub const ACTION_REDO_CHECKPOINT: &str = "timeline.redo";

pub const HOTKEY_ACTIONS: &[&str] = &[
    ACTION_APP_TOGGLE,
    ACTION_CREATE_ZONE,
    ACTION_DUPLICATE_ZONE,
    ACTION_ZONE_LOCK_TOGGLE,
    ACTION_ZONE_HIDE_ALL,
    ACTION_LAYOUT_AUTO_ORGANIZE,
    ACTION_LAYOUT_REFLOW,
    ACTION_BULK_OPEN_MANAGER,
    ACTION_ZONE_FOCUS_NEXT,
    ACTION_ZONE_FOCUS_PREV,
    ACTION_TOGGLE_SETTINGS,
    ACTION_OPEN_SEARCH,
    ACTION_QUIT_APP,
    ACTION_OPEN_TIMELINE,
    ACTION_OPEN_SNAPSHOT_PICKER,
    ACTION_UNDO_CHECKPOINT,
    ACTION_REDO_CHECKPOINT,
];

/// Default in-process bindings. Zero allocation; linear scan in [`lookup`].
pub const DEFAULT_BINDINGS: &[HotkeyBinding] = &[
    HotkeyBinding {
        vk: VK_ESCAPE as u32,
        mods: ModFlags::none(),
        command: HotkeyCommand::Escape,
    },
    HotkeyBinding {
        vk: VK_SPACE as u32,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::ToggleMain,
    },
    HotkeyBinding {
        vk: VK_N as u32,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::CreateZone,
    },
    HotkeyBinding {
        vk: VK_D_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::DuplicateZone,
    },
    HotkeyBinding {
        vk: VK_L_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::ToggleZoneLock,
    },
    HotkeyBinding {
        vk: VK_H_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::ToggleAllZones,
    },
    HotkeyBinding {
        vk: VK_O_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::AutoOrganize,
    },
    HotkeyBinding {
        vk: VK_R_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::ReflowLayout,
    },
    HotkeyBinding {
        vk: VK_M_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::OpenBulkManager,
    },
    HotkeyBinding {
        vk: VK_OEM_6_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::FocusNextZone,
    },
    HotkeyBinding {
        vk: VK_OEM_4_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::FocusPreviousZone,
    },
    HotkeyBinding {
        vk: VK_OEM_COMMA as u32,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::ToggleSettings,
    },
    HotkeyBinding {
        vk: VK_K_KEY,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::OpenSearch,
    },
    HotkeyBinding {
        vk: VK_Q as u32,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::QuitApp,
    },
    HotkeyBinding {
        vk: VK_T as u32,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::OpenTimeline,
    },
    HotkeyBinding {
        vk: VK_S as u32,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::OpenSnapshotPicker,
    },
    HotkeyBinding {
        vk: VK_Z as u32,
        mods: ModFlags {
            ctrl: true,
            shift: false,
            alt: false,
        },
        command: HotkeyCommand::UndoCheckpoint,
    },
    HotkeyBinding {
        vk: VK_Z as u32,
        mods: ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
        command: HotkeyCommand::RedoCheckpoint,
    },
];

/// Resolve `(vk, mods)` to the configured `HotkeyCommand`, if any. Linear
/// scan over [`DEFAULT_BINDINGS`] — the table is intentionally tiny, so a
/// HashMap would be net loss (heap alloc + hash compute > a few cmp ops).
pub fn lookup(vk: u32, mods: ModFlags) -> Option<HotkeyCommand> {
    lookup_in(DEFAULT_BINDINGS, vk, mods)
}

/// Resolve against an explicit binding table. The shell uses this after
/// merging persisted config-vault overrides with [`DEFAULT_BINDINGS`].
pub fn lookup_in(bindings: &[HotkeyBinding], vk: u32, mods: ModFlags) -> Option<HotkeyCommand> {
    for b in bindings {
        if b.vk == vk && b.mods == mods {
            return Some(b.command);
        }
    }
    None
}

/// Convert `keybinding.<action> = "Ctrl+Shift+S"` into a runtime binding.
pub fn binding_for_action(action: &str, chord: &str) -> Option<HotkeyBinding> {
    let command = command_for_action(action)?;
    let (vk, mods) = parse_chord(chord)?;
    Some(HotkeyBinding { vk, mods, command })
}

/// Validate a `(action, chord)` pair against an existing runtime table without
/// mutating it. Used by the Settings recorder to surface conflicts before
/// persisting `keybinding.*` into the config vault.
pub fn validate_binding(
    bindings: &[HotkeyBinding],
    action: &str,
    chord: &str,
) -> Result<HotkeyBinding, BindingValidationError> {
    let Some(binding) = binding_for_action(action, chord) else {
        return Err(BindingValidationError::UnsupportedActionOrChord);
    };
    if bindings.iter().any(|existing| {
        existing.command != binding.command
            && existing.vk == binding.vk
            && existing.mods == binding.mods
    }) {
        return Err(BindingValidationError::ChordAlreadyAssigned);
    }
    Ok(binding)
}

pub fn command_for_action(action: &str) -> Option<HotkeyCommand> {
    match action {
        ACTION_APP_TOGGLE => Some(HotkeyCommand::ToggleMain),
        ACTION_CREATE_ZONE => Some(HotkeyCommand::CreateZone),
        ACTION_DUPLICATE_ZONE => Some(HotkeyCommand::DuplicateZone),
        ACTION_ZONE_LOCK_TOGGLE => Some(HotkeyCommand::ToggleZoneLock),
        ACTION_ZONE_HIDE_ALL => Some(HotkeyCommand::ToggleAllZones),
        ACTION_LAYOUT_AUTO_ORGANIZE => Some(HotkeyCommand::AutoOrganize),
        ACTION_LAYOUT_REFLOW => Some(HotkeyCommand::ReflowLayout),
        ACTION_BULK_OPEN_MANAGER => Some(HotkeyCommand::OpenBulkManager),
        ACTION_ZONE_FOCUS_NEXT => Some(HotkeyCommand::FocusNextZone),
        ACTION_ZONE_FOCUS_PREV => Some(HotkeyCommand::FocusPreviousZone),
        ACTION_TOGGLE_SETTINGS => Some(HotkeyCommand::ToggleSettings),
        ACTION_OPEN_SEARCH => Some(HotkeyCommand::OpenSearch),
        ACTION_QUIT_APP => Some(HotkeyCommand::QuitApp),
        ACTION_OPEN_TIMELINE => Some(HotkeyCommand::OpenTimeline),
        ACTION_OPEN_SNAPSHOT_PICKER => Some(HotkeyCommand::OpenSnapshotPicker),
        ACTION_UNDO_CHECKPOINT => Some(HotkeyCommand::UndoCheckpoint),
        ACTION_REDO_CHECKPOINT => Some(HotkeyCommand::RedoCheckpoint),
        _ => None,
    }
}

pub fn default_chord_for_action(action: &str) -> Option<&'static str> {
    match action {
        ACTION_APP_TOGGLE => Some("Control+Space"),
        ACTION_CREATE_ZONE => Some("Control+Shift+N"),
        ACTION_DUPLICATE_ZONE => Some("Control+Shift+D"),
        ACTION_ZONE_LOCK_TOGGLE => Some("Control+Shift+L"),
        ACTION_ZONE_HIDE_ALL => Some("Control+Shift+H"),
        ACTION_LAYOUT_AUTO_ORGANIZE => Some("Control+Shift+O"),
        ACTION_LAYOUT_REFLOW => Some("Control+Shift+R"),
        ACTION_BULK_OPEN_MANAGER => Some("Control+Shift+M"),
        ACTION_ZONE_FOCUS_NEXT => Some("Control+]"),
        ACTION_ZONE_FOCUS_PREV => Some("Control+["),
        ACTION_TOGGLE_SETTINGS => Some("Ctrl+,"),
        ACTION_OPEN_SEARCH => Some("Ctrl+K"),
        ACTION_QUIT_APP => Some("Ctrl+Q"),
        ACTION_OPEN_TIMELINE => Some("Ctrl+T"),
        ACTION_OPEN_SNAPSHOT_PICKER => Some("Ctrl+Shift+S"),
        ACTION_UNDO_CHECKPOINT => Some("Ctrl+Z"),
        ACTION_REDO_CHECKPOINT => Some("Ctrl+Shift+Z"),
        _ => None,
    }
}

/// Format a Win32 keydown into the selected-stack chord syntax accepted by
/// [`binding_for_action`]. Modifier-only keydowns return `None` so recording
/// remains active until the user presses a real key.
pub fn format_chord(vk: u32, mods: ModFlags) -> Option<String> {
    if vk == VK_CONTROL as u32 || vk == VK_SHIFT as u32 || vk == VK_MENU as u32 {
        return None;
    }
    let key = key_name_for_vk(vk)?;
    let mut chord = String::new();
    if mods.ctrl {
        chord.push_str("Ctrl+");
    }
    if mods.alt {
        chord.push_str("Alt+");
    }
    if mods.shift {
        chord.push_str("Shift+");
    }
    chord.push_str(key);
    Some(chord)
}

fn key_name_for_vk(vk: u32) -> Option<&'static str> {
    if vk == VK_ESCAPE as u32 {
        Some("Escape")
    } else if vk == VK_OEM_COMMA as u32 {
        Some(",")
    } else if vk == VK_OEM_4_KEY {
        Some("[")
    } else if vk == VK_OEM_6_KEY {
        Some("]")
    } else if vk == VK_SPACE as u32 {
        Some("Space")
    } else if vk == VK_TAB as u32 {
        Some("Tab")
    } else if vk == VK_F4 as u32 {
        Some("F4")
    } else if (0x41..=0x5A).contains(&vk) {
        match vk {
            0x41 => Some("A"),
            0x42 => Some("B"),
            0x43 => Some("C"),
            0x44 => Some("D"),
            0x45 => Some("E"),
            0x46 => Some("F"),
            0x47 => Some("G"),
            0x48 => Some("H"),
            0x49 => Some("I"),
            0x4A => Some("J"),
            0x4B => Some("K"),
            0x4C => Some("L"),
            0x4D => Some("M"),
            0x4E => Some("N"),
            0x4F => Some("O"),
            0x50 => Some("P"),
            0x51 => Some("Q"),
            0x52 => Some("R"),
            0x53 => Some("S"),
            0x54 => Some("T"),
            0x55 => Some("U"),
            0x56 => Some("V"),
            0x57 => Some("W"),
            0x58 => Some("X"),
            0x59 => Some("Y"),
            0x5A => Some("Z"),
            _ => None,
        }
    } else {
        None
    }
}

fn parse_chord(chord: &str) -> Option<(u32, ModFlags)> {
    let mut mods = ModFlags::none();
    let mut key = None::<&str>;
    for part in chord.split('+') {
        let token = part.trim();
        if token.eq_ignore_ascii_case("ctrl") || token.eq_ignore_ascii_case("control") {
            mods.ctrl = true;
        } else if token.eq_ignore_ascii_case("shift") {
            mods.shift = true;
        } else if token.eq_ignore_ascii_case("alt") {
            mods.alt = true;
        } else if token.is_empty() || key.replace(token).is_some() {
            return None;
        }
    }
    let key = key?;
    let vk = if key.eq_ignore_ascii_case("escape") || key.eq_ignore_ascii_case("esc") {
        VK_ESCAPE as u32
    } else if key == "," {
        VK_OEM_COMMA as u32
    } else if key == "[" {
        VK_OEM_4_KEY
    } else if key == "]" {
        VK_OEM_6_KEY
    } else if key.eq_ignore_ascii_case("space") {
        VK_SPACE as u32
    } else if key.eq_ignore_ascii_case("tab") {
        VK_TAB as u32
    } else if key.eq_ignore_ascii_case("f4") {
        VK_F4 as u32
    } else if key.len() == 1 {
        let ch = key.as_bytes()[0];
        if ch.is_ascii_alphabetic() {
            ch.to_ascii_uppercase() as u32
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some((vk, mods))
}
