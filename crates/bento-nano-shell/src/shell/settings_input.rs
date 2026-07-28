//! Native shell owner: `settings_input`.

use super::*;

pub(super) fn handle_settings_passphrase_keydown(
    root: &AppRoot,
    vk: u32,
    hwnd: HWND,
) -> Option<LRESULT> {
    let active = {
        let app = root.app.borrow();
        app.settings_open.get() && app.passphrase_entry_active.get()
    };
    if !active {
        return None;
    }
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            let _ = app.passphrase_draft.borrow_mut().pop();
            request_redraw(hwnd);
            Some(0)
        }
        VK_ENTER => {
            let (passphrase, purpose) = {
                let app = root.app.borrow();
                let draft = app.passphrase_draft.borrow().trim().to_owned();
                if draft.is_empty() {
                    app.settings_encryption_status.borrow_mut().replace(
                        SettingsBackupStatus::Error(localized_current(
                            "请输入口令",
                            "Passphrase required",
                        )),
                    );
                    request_redraw(hwnd);
                    return Some(0);
                }
                let purpose = app.passphrase_entry_purpose.get();
                app.passphrase_entry_active.set(false);
                app.passphrase_draft.borrow_mut().clear();
                (draft, purpose)
            };
            let command = match purpose {
                PassphraseEntryPurpose::Set => {
                    Command::SetEncryptionPassphrase(SmolStr::new(passphrase))
                }
                PassphraseEntryPurpose::Unlock => {
                    Command::UnlockEncryptionPassphrase(SmolStr::new(passphrase))
                }
            };
            root.dispatcher.push(command);
            request_redraw(hwnd);
            Some(0)
        }
        VK_ESCAPE_KEY => {
            let app = root.app.borrow();
            app.passphrase_entry_active.set(false);
            app.passphrase_draft.borrow_mut().clear();
            app.settings_encryption_status
                .borrow_mut()
                .replace(SettingsBackupStatus::Success(localized_current(
                    "已取消口令输入",
                    "Passphrase entry cancelled",
                )));
            request_redraw(hwnd);
            Some(0)
        }
        _ => None,
    }
}

pub(super) fn handle_settings_keybinding_keydown(
    root: &AppRoot,
    vk: u32,
    hwnd: HWND,
) -> Option<LRESULT> {
    let (modal_open, recording_action) = {
        let app = root.app.borrow();
        (
            app.settings_open.get() && app.settings_keybindings_open.get(),
            app.settings_keybinding_recording.borrow().clone(),
        )
    };
    if !modal_open {
        return None;
    }
    let Some(action) = recording_action else {
        if vk == VK_ESCAPE_KEY {
            let app = root.app.borrow();
            app.settings_keybindings_open.set(false);
            app.settings_keybinding_recording.borrow_mut().take();
            request_redraw(hwnd);
        }
        return Some(0);
    };
    if vk == VK_ESCAPE_KEY {
        let app = root.app.borrow();
        app.settings_keybinding_recording.borrow_mut().take();
        set_keybinding_feedback(
            &app,
            action.as_str(),
            localized_current("已取消快捷键录制", "Shortcut recording cancelled"),
            false,
        );
        request_redraw(hwnd);
        return Some(0);
    }
    let mods = hotkey::ModFlags::from_keystate();
    let Some(chord) = hotkey::format_chord(vk, mods) else {
        return Some(0);
    };
    let app = root.app.borrow();
    app.settings_keybinding_recording.borrow_mut().take();
    if keybindings_section::is_reserved_chord(chord.as_str()) {
        set_keybinding_feedback(
            &app,
            action.as_str(),
            localized_current("该组合键由 Windows 保留", "Reserved by Windows"),
            true,
        );
        request_redraw(hwnd);
        return Some(0);
    }
    match validate_keybinding_candidate(root, action.as_str(), chord.as_str()) {
        Ok(()) => {
            let Some(key) = keybinding_setting_key(action.as_str()) else {
                set_keybinding_feedback(
                    &app,
                    action.as_str(),
                    localized_current("不支持此操作", "Unsupported action"),
                    true,
                );
                request_redraw(hwnd);
                return Some(0);
            };
            set_keybinding_feedback(
                &app,
                action.as_str(),
                localized_current(format!("正在保存 {chord}"), format!("Saving {chord}")),
                false,
            );
            drop(app);
            root.dispatcher.push(Command::SetSetting {
                key,
                value: bento_nano_app::SettingValue::Str(SmolStr::new(chord)),
            });
        }
        Err(hotkey::BindingValidationError::UnsupportedActionOrChord) => {
            set_keybinding_feedback(
                &app,
                action.as_str(),
                localized_current("不支持此快捷键", "Unsupported shortcut"),
                true,
            );
        }
        Err(hotkey::BindingValidationError::ChordAlreadyAssigned) => {
            set_keybinding_feedback(
                &app,
                action.as_str(),
                localized_current("该快捷键已被使用", "Already in use"),
                true,
            );
        }
    }
    request_redraw(hwnd);
    Some(0)
}

// M1h (2026-05-29) — `handle_settings_plugins_keydown` removed. It existed only
// to close the (now-deleted) plugins MODAL on Esc; the inline §11 Plugins
// section has no modal, and Esc on the Settings panel already closes the whole
// surface via the auxiliary-escape path.

pub(super) fn handle_settings_passphrase_char(root: &AppRoot, codepoint: u32) -> bool {
    if matches!(codepoint, VK_BACKSPACE | VK_ENTER | VK_ESCAPE_KEY) {
        return false;
    }
    let Some(ch) = char::from_u32(codepoint) else {
        return false;
    };
    if ch.is_control() {
        return false;
    }
    let app = root.app.borrow();
    if !app.settings_open.get() || !app.passphrase_entry_active.get() {
        return false;
    }
    let mut draft = app.passphrase_draft.borrow_mut();
    if draft.chars().count() < 128 {
        draft.push(ch);
    }
    true
}

/// M7 (2026-06-01) — WM_CHAR handler for the §2 妗岄潰璺緞 / 鐩戞帶鍊?inline text
/// fields (the focused-field model that generalises the passphrase-only path).
/// Gated on `settings_open && settings_focused_field 鈭?{DesktopPath,
/// WatchValues}`; appends the composed Unicode codepoint to the focused draft
/// via the pure `AppState::settings_focused_push_char` (which caps length, is
/// CJK-safe, and allows `\n` only for the WatchValues textarea). Marks the
/// panel dirty so Save lights up. Returns `true` when handled (so the WM_CHAR
/// dispatcher stops + redraws). The Passphrase field is NOT handled here — it
/// keeps `handle_settings_passphrase_char` + commit-on-Enter. CJK arrives as a
/// post-composition codepoint via WM_CHAR (no WM_IME_* needed — same as every
/// existing nano text field).
pub(super) fn handle_settings_text_char(root: &AppRoot, codepoint: u32) -> bool {
    // Reject the control codepoints the keydown path owns (Backspace/Enter/Esc)
    // so they never double-route into the draft.
    if matches!(codepoint, VK_BACKSPACE | VK_ENTER | VK_ESCAPE_KEY) {
        return false;
    }
    let Some(ch) = char::from_u32(codepoint) else {
        return false;
    };
    let app = root.app.borrow();
    if !app.settings_open.get() {
        return false;
    }
    if !matches!(
        app.settings_focused_field.get(),
        bento_nano_app::SettingsTextField::DesktopPath
            | bento_nano_app::SettingsTextField::WatchValues
            | bento_nano_app::SettingsTextField::AccentColor
    ) {
        return false;
    }
    if app.settings_focused_push_char(ch) {
        app.settings_dirty.set(true);
        true
    } else {
        // Field is focused but the char was rejected (control / over cap). Still
        // consume it so it never leaks to DefWindowProc.
        true
    }
}

/// M7 — WM_KEYDOWN handler for the §2 妗岄潰璺緞 / 鐩戞帶鍊?inline text fields.
/// Returns `Some(0)` (consumed) ONLY when a non-passphrase field is focused;
/// otherwise `None` so the passphrase + auxiliary-escape paths still run.
/// - Backspace → pop last scalar, mark dirty, redraw.
/// - Enter → DesktopPath (single line): blur the field; WatchValues (textarea):
///   insert a `\n` into the draft.
/// - Esc → blur the field (set `None`) + return `None`, allowing the same
///   keydown to reach the Settings auxiliary-escape close/cancel path.
pub(super) fn handle_settings_text_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> Option<LRESULT> {
    let field = {
        let app = root.app.borrow();
        if !app.settings_open.get() {
            return None;
        }
        app.settings_focused_field.get()
    };
    let is_text_field = matches!(
        field,
        bento_nano_app::SettingsTextField::DesktopPath
            | bento_nano_app::SettingsTextField::WatchValues
            | bento_nano_app::SettingsTextField::AccentColor
    );
    if !is_text_field {
        return None;
    }
    match vk {
        VK_BACKSPACE => {
            let app = root.app.borrow();
            if app.settings_focused_backspace() {
                app.settings_dirty.set(true);
            }
            drop(app);
            request_redraw(hwnd);
            Some(0)
        }
        VK_ENTER => {
            let app = root.app.borrow();
            if matches!(field, bento_nano_app::SettingsTextField::WatchValues) {
                // Textarea: Enter inserts a newline (one watch path per line).
                if app.settings_focused_push_char('\n') {
                    app.settings_dirty.set(true);
                }
            } else {
                // Single-line input: Enter blurs the field.
                app.settings_focused_field
                    .set(bento_nano_app::SettingsTextField::None);
            }
            drop(app);
            request_redraw(hwnd);
            Some(0)
        }
        VK_ESCAPE_KEY => {
            // Blur first, then let this same keydown close/cancel Settings.
            let app = root.app.borrow();
            app.settings_focused_field
                .set(bento_nano_app::SettingsTextField::None);
            drop(app);
            request_redraw(hwnd);
            None
        }
        _ => None,
    }
}
