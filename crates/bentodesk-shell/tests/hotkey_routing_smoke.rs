//! Phase 2.2 / Ruling 1 — hotkey lookup smoke tests.
//!
//! These tests cover only the `lookup` table; ESC context resolution
//! (settings_open vs hide window) lives in `wnd_proc::handle_keydown` and
//! is exercised by the matching tests in `phase22_smoke.rs`.

#![forbid(unsafe_op_in_unsafe_fn)]

use bentodesk_shell::hotkey::{
    ACTION_APP_TOGGLE, ACTION_BULK_OPEN_MANAGER, ACTION_CREATE_ZONE, ACTION_DUPLICATE_ZONE,
    ACTION_LAYOUT_REFLOW, ACTION_OPEN_SEARCH, ACTION_OPEN_SNAPSHOT_PICKER, ACTION_OPEN_TIMELINE,
    ACTION_REDO_CHECKPOINT, ACTION_ZONE_FOCUS_NEXT, ACTION_ZONE_FOCUS_PREV, ACTION_ZONE_HIDE_ALL,
    ACTION_ZONE_LOCK_TOGGLE, BindingValidationError, HotkeyCommand, ModFlags, binding_for_action,
    default_chord_for_action, format_chord, lookup, lookup_in, validate_binding,
};

const VK_ESCAPE: u32 = 0x1B;
const VK_D: u32 = 0x44;
const VK_H: u32 = 0x48;
const VK_K: u32 = 0x4B;
const VK_L: u32 = 0x4C;
const VK_M: u32 = 0x4D;
const VK_N: u32 = 0x4E;
const VK_R: u32 = 0x52;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_4: u32 = 0xDB;
const VK_OEM_6: u32 = 0xDD;
const VK_Q: u32 = 0x51;
const VK_S: u32 = 0x53;
const VK_T: u32 = 0x54;
const VK_Z: u32 = 0x5A;
const VK_F1: u32 = 0x70; // unbound

#[test]
fn escape_no_mods_resolves_to_escape_command() {
    assert_eq!(
        lookup(VK_ESCAPE, ModFlags::none()),
        Some(HotkeyCommand::Escape)
    );
}

#[test]
fn ctrl_shift_n_resolves_to_create_zone() {
    let mods = ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    };
    assert_eq!(lookup(VK_N, mods), Some(HotkeyCommand::CreateZone));
}

#[test]
fn tauri_default_shortcuts_resolve_to_selected_stack_commands() {
    let ctrl_shift = ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    };
    let ctrl = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };

    assert_eq!(
        lookup(VK_M, ctrl_shift),
        Some(HotkeyCommand::OpenBulkManager)
    );
    assert_eq!(lookup(VK_D, ctrl_shift), Some(HotkeyCommand::DuplicateZone));
    assert_eq!(
        lookup(VK_L, ctrl_shift),
        Some(HotkeyCommand::ToggleZoneLock)
    );
    assert_eq!(
        lookup(VK_H, ctrl_shift),
        Some(HotkeyCommand::ToggleAllZones)
    );
    assert_eq!(lookup(VK_R, ctrl_shift), Some(HotkeyCommand::ReflowLayout));
    assert_eq!(lookup(VK_OEM_6, ctrl), Some(HotkeyCommand::FocusNextZone));
    assert_eq!(
        lookup(VK_OEM_4, ctrl),
        Some(HotkeyCommand::FocusPreviousZone)
    );
}

#[test]
fn ctrl_comma_resolves_to_toggle_settings() {
    let mods = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    assert_eq!(
        lookup(VK_OEM_COMMA, mods),
        Some(HotkeyCommand::ToggleSettings)
    );
}

#[test]
fn ctrl_k_resolves_to_open_search() {
    let mods = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    assert_eq!(lookup(VK_K, mods), Some(HotkeyCommand::OpenSearch));
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(lookup(VK_F1, ModFlags::none()), None);
}

#[test]
fn ctrl_q_resolves_to_quit_app() {
    let mods = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    assert_eq!(lookup(VK_Q, mods), Some(HotkeyCommand::QuitApp));
}

#[test]
fn ctrl_t_resolves_to_open_timeline() {
    let mods = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    assert_eq!(lookup(VK_T, mods), Some(HotkeyCommand::OpenTimeline));
}

#[test]
fn ctrl_shift_s_resolves_to_open_snapshot_picker() {
    let mods = ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    };
    assert_eq!(lookup(VK_S, mods), Some(HotkeyCommand::OpenSnapshotPicker));
}

#[test]
fn ctrl_z_resolves_to_undo_checkpoint() {
    let mods = ModFlags {
        ctrl: true,
        shift: false,
        alt: false,
    };
    assert_eq!(lookup(VK_Z, mods), Some(HotkeyCommand::UndoCheckpoint));
}

#[test]
fn ctrl_shift_z_resolves_to_redo_checkpoint() {
    let mods = ModFlags {
        ctrl: true,
        shift: true,
        alt: false,
    };
    assert_eq!(lookup(VK_Z, mods), Some(HotkeyCommand::RedoCheckpoint));
}

#[test]
fn persisted_action_chord_builds_runtime_binding() {
    let binding = binding_for_action(ACTION_OPEN_TIMELINE, "Ctrl+Shift+T");
    assert!(binding.is_some(), "runtime binding");
    let Some(binding) = binding else {
        return;
    };
    assert_eq!(binding.vk, VK_T);
    assert_eq!(
        binding.mods,
        ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        }
    );
    assert_eq!(
        lookup_in(&[binding], VK_T, binding.mods),
        Some(HotkeyCommand::OpenTimeline)
    );
}

#[test]
fn unsupported_action_or_chord_is_rejected() {
    assert!(binding_for_action("zone.unimplemented", "Ctrl+D").is_none());
    assert!(binding_for_action(ACTION_OPEN_TIMELINE, "Ctrl+Mouse1").is_none());
}

#[test]
fn default_chords_are_available_for_runtime_actions() {
    assert_eq!(
        default_chord_for_action(ACTION_REDO_CHECKPOINT),
        Some("Ctrl+Shift+Z")
    );
    assert_eq!(
        default_chord_for_action(ACTION_APP_TOGGLE),
        Some("Control+Space")
    );
    assert_eq!(
        default_chord_for_action(ACTION_CREATE_ZONE),
        Some("Control+Shift+N")
    );
    assert_eq!(
        default_chord_for_action(ACTION_BULK_OPEN_MANAGER),
        Some("Control+Shift+M")
    );
    assert_eq!(
        default_chord_for_action(ACTION_DUPLICATE_ZONE),
        Some("Control+Shift+D")
    );
    assert_eq!(
        default_chord_for_action(ACTION_ZONE_LOCK_TOGGLE),
        Some("Control+Shift+L")
    );
    assert_eq!(
        default_chord_for_action(ACTION_ZONE_HIDE_ALL),
        Some("Control+Shift+H")
    );
    assert_eq!(
        default_chord_for_action(ACTION_LAYOUT_REFLOW),
        Some("Control+Shift+R")
    );
    assert_eq!(
        default_chord_for_action(ACTION_ZONE_FOCUS_NEXT),
        Some("Control+]")
    );
    assert_eq!(
        default_chord_for_action(ACTION_ZONE_FOCUS_PREV),
        Some("Control+[")
    );
    assert_eq!(default_chord_for_action(ACTION_OPEN_SEARCH), Some("Ctrl+K"));
}

#[test]
fn recorder_formats_supported_win32_keydown_to_chord() {
    let chord = format_chord(
        VK_T,
        ModFlags {
            ctrl: true,
            shift: true,
            alt: false,
        },
    );
    assert_eq!(chord.as_deref(), Some("Ctrl+Shift+T"));
}

#[test]
fn recorder_validation_rejects_runtime_conflict() {
    let binding = binding_for_action(ACTION_OPEN_TIMELINE, "Ctrl+Shift+T");
    assert!(binding.is_some(), "timeline binding");
    let Some(binding) = binding else {
        return;
    };
    assert_eq!(
        validate_binding(&[binding], ACTION_OPEN_SNAPSHOT_PICKER, "Ctrl+Shift+T"),
        Err(BindingValidationError::ChordAlreadyAssigned)
    );
}
